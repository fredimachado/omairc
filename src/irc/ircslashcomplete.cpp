#include "ircslashcomplete.h"

#include <QMetaType>
#include <QVariantMap>
#include <algorithm>
#include <utility>

IrcSlashProbe IrcSlashProbe::closed()
{
    return {};
}

IrcSlashProbe IrcSlashProbe::open(QString needle, QVector<IrcSlashHit> hits)
{
    if (hits.isEmpty() || needle.isEmpty())
        return closed();
    IrcSlashProbe probe;
    probe.m_open = true;
    probe.m_needle = std::move(needle);
    probe.m_hits = std::move(hits);
    return probe;
}

bool IrcSlashProbe::containsLabel(const QString& label) const
{
    for (const IrcSlashHit& hit : m_hits) {
        if (hit.label == label)
            return true;
    }
    return false;
}

QString IrcSlashComplete::openableNeedle(const QString& composerText)
{
    int start = 0;
    while (start < composerText.size() && composerText.at(start).isSpace())
        ++start;
    if (start >= composerText.size())
        return {};
    if (composerText.at(start) != QLatin1Char('/'))
        return {};
    const int afterSlash = start + 1;
    if (afterSlash >= composerText.size())
        return {};
    if (composerText.at(afterSlash) == QLatin1Char('/')
        || composerText.at(afterSlash).isSpace()) {
        return {};
    }
    for (int i = afterSlash; i < composerText.size(); ++i) {
        if (composerText.at(i).isSpace())
            return {};
    }
    return composerText.mid(afterSlash).toLower();
}

int IrcSlashComplete::scoreToken(const QString& foldedNeedle, const QString& token)
{
    if (foldedNeedle.isEmpty() || token.isEmpty())
        return -1;
    if (foldedNeedle.at(0) != token.at(0))
        return -1;
    if (foldedNeedle == token)
        return 100;
    if (token.startsWith(foldedNeedle))
        return 80;
    if (token.contains(foldedNeedle))
        return 50;
    int ti = 0;
    for (const QChar ch : foldedNeedle) {
        ti = token.indexOf(ch, ti);
        if (ti < 0)
            return -1;
        ++ti;
    }
    return 20 + (foldedNeedle.size() * 10) / ti;
}

int IrcSlashComplete::scoreSpec(const QString& foldedNeedle, const IrcVerbSpec& spec)
{
    int best = scoreToken(foldedNeedle, spec.name);
    for (const QString& alias : spec.aliases)
        best = std::max(best, scoreToken(foldedNeedle, alias));
    return best;
}

QVector<IrcSlashHit> IrcSlashComplete::rank(const QString& foldedNeedle,
                                            IrcComposerSurface surface)
{
    const QVector<IrcVerbSpec> visible = IrcVerbTable::visibleOn(surface);
    struct Scored {
        int catalogIndex = 0;
        int score = -1;
        IrcSlashHit hit;
    };
    QVector<Scored> scored;
    for (int i = 0; i < visible.size(); ++i) {
        const IrcVerbSpec& spec = visible.at(i);
        const int score = scoreSpec(foldedNeedle, spec);
        if (score < 0)
            continue;
        scored.append({i, score,
                       {QLatin1Char('/') + spec.name, spec.usage}});
    }
    std::stable_sort(scored.begin(), scored.end(),
                     [](const Scored& left, const Scored& right) {
        if (left.score != right.score)
            return left.score > right.score;
        return left.catalogIndex < right.catalogIndex;
    });
    QVector<IrcSlashHit> hits;
    const int keep = std::min(int(scored.size()), 6);
    hits.reserve(keep);
    for (int i = 0; i < keep; ++i)
        hits.append(scored.at(i).hit);
    return hits;
}

IrcSlashProbe IrcSlashComplete::project(const QString& composerText,
                                        IrcComposerSurface surface)
{
    const QString needle = openableNeedle(composerText);
    if (needle.isEmpty())
        return IrcSlashProbe::closed();
    return IrcSlashProbe::open(needle, rank(needle, surface));
}

IrcSlashSession::IrcSlashSession(QObject *parent)
    : QObject(parent)
{
    qRegisterMetaType<IrcSlashKeyResult>();
}

bool IrcSlashSession::open() const
{
    return m_probe.isOpen();
}

QVariantList IrcSlashSession::matches() const
{
    QVariantList rows;
    for (const IrcSlashHit& hit : m_probe.hits()) {
        QVariantMap row;
        row.insert(QStringLiteral("label"), hit.label);
        row.insert(QStringLiteral("usage"), hit.usage);
        rows.append(row);
    }
    return rows;
}

int IrcSlashSession::selectedIndex() const
{
    return m_selectedIndex;
}

void IrcSlashSession::setSelectedIndex(int index)
{
    if (!open())
        return;
    const int clamped = qBound(0, index, m_probe.hits().size() - 1);
    if (clamped == m_selectedIndex)
        return;
    m_selectedIndex = clamped;
    emit snapshotChanged();
}

void IrcSlashSession::sync(const QString& composerText, bool statusConsole)
{
    m_lastText = composerText;
    const auto surface = statusConsole
        ? IrcComposerSurface::Status
        : IrcComposerSurface::Conversation;
    applyProbe(IrcSlashComplete::project(composerText, surface));
}

QString IrcSlashSession::insertionAt(int index) const
{
    if (!open() || index < 0 || index >= m_probe.hits().size())
        return {};
    return m_probe.hits().at(index).label + QLatin1Char(' ');
}

QString IrcSlashSession::replacedComposer(int index) const
{
    const QString insertion = insertionAt(index);
    if (insertion.isEmpty())
        return {};
    int start = 0;
    while (start < m_lastText.size() && m_lastText.at(start).isSpace())
        ++start;
    return m_lastText.left(start) + insertion;
}

bool IrcSlashSession::tokenPassesSelected() const
{
    if (!open() || m_selectedIndex < 0 || m_selectedIndex >= m_probe.hits().size())
        return false;
    const QString needle = m_probe.needle();
    const QString name = m_probe.hits().at(m_selectedIndex).label.mid(1);
    if (needle == name)
        return true;
    const IrcVerbSpec *spec = IrcVerbTable::lookup(name);
    return spec && spec->aliases.contains(needle);
}

void IrcSlashSession::applyProbe(const IrcSlashProbe& probe)
{
    if (!probe.isOpen()) {
        if (!m_dismissedNeedle.isEmpty() && m_dismissedNeedle != probe.needle())
            m_dismissedNeedle.clear();
        const bool changed = m_probe.isOpen() || m_selectedIndex != -1;
        m_probe = IrcSlashProbe::closed();
        m_selectedIndex = -1;
        if (changed)
            emit snapshotChanged();
        return;
    }

    if (probe.needle() == m_dismissedNeedle) {
        if (m_probe.isOpen() || m_selectedIndex != -1) {
            m_probe = IrcSlashProbe::closed();
            m_selectedIndex = -1;
            emit snapshotChanged();
        }
        return;
    }

    m_dismissedNeedle.clear();
    const bool needleChanged = !m_probe.isOpen() || m_probe.needle() != probe.needle();
    const int nextIndex = needleChanged
        ? 0
        : qBound(0, m_selectedIndex, probe.hits().size() - 1);
    const bool hitsChanged = m_probe.hits() != probe.hits();
    const bool opened = !m_probe.isOpen();
    const bool selectionChanged = nextIndex != m_selectedIndex;
    m_probe = probe;
    m_selectedIndex = nextIndex;
    if (opened || needleChanged || hitsChanged || selectionChanged)
        emit snapshotChanged();
}

IrcSlashKeyResult IrcSlashSession::routeKey(int key, int modifiers)
{
    IrcSlashKeyResult result;
    const Qt::KeyboardModifiers mods = Qt::KeyboardModifiers(modifiers);
    if (mods != Qt::NoModifier && mods != Qt::KeypadModifier)
        return result;
    if (!open())
        return result;

    const int hitCount = m_probe.hits().size();
    switch (key) {
    case Qt::Key_Escape:
        dismiss();
        result.accepted = true;
        return result;
    case Qt::Key_Up:
        setSelectedIndex((m_selectedIndex + hitCount - 1) % hitCount);
        result.accepted = true;
        return result;
    case Qt::Key_Down:
        setSelectedIndex((m_selectedIndex + 1) % hitCount);
        result.accepted = true;
        return result;
    case Qt::Key_Tab:
        result.accepted = true;
        result.insertion = replacedComposer(m_selectedIndex);
        return result;
    case Qt::Key_Enter:
    case Qt::Key_Return:
        if (tokenPassesSelected())
            return result;
        result.accepted = true;
        result.insertion = replacedComposer(m_selectedIndex);
        return result;
    default:
        return result;
    }
}

QString IrcSlashSession::activate(int index)
{
    if (!open())
        return {};
    setSelectedIndex(index);
    return replacedComposer(m_selectedIndex);
}

void IrcSlashSession::dismiss()
{
    if (!open())
        return;
    m_dismissedNeedle = m_probe.needle();
    m_probe = IrcSlashProbe::closed();
    m_selectedIndex = -1;
    emit snapshotChanged();
}
