#include "irchighlight.h"

#include <QByteArray>
#include <QSettings>

#include <string>

namespace
{
const auto highlightsGroup = QStringLiteral("highlights");
const auto wordsKey = QStringLiteral("words");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool wordEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

int indexOfWord(const QStringList& words,
                const QString& word,
                const IrcCaseMapping& mapping)
{
    for (int i = 0; i < words.size(); ++i) {
        if (wordEquals(mapping, words.at(i), word))
            return i;
    }
    return -1;
}

bool listContains(const QStringList& words,
                  const QString& word,
                  const IrcCaseMapping& mapping)
{
    return indexOfWord(words, word, mapping) >= 0;
}
}

QStringList IrcHighlightStore::load(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    QSettings settings;
    settings.beginGroup(highlightsGroup);
    settings.beginGroup(networkId);
    return settings.value(wordsKey).toStringList();
}

void IrcHighlightStore::save(const QString& networkId, const QStringList& words)
{
    if (networkId.isEmpty())
        return;
    QSettings settings;
    settings.beginGroup(highlightsGroup);
    if (words.isEmpty()) {
        settings.remove(networkId);
    } else {
        settings.beginGroup(networkId);
        settings.setValue(wordsKey, words);
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}

const QStringList& IrcHighlightStore::cached(const QString& networkId) const
{
    const auto found = m_cache.constFind(networkId);
    if (found != m_cache.cend())
        return found.value();
    return m_cache.insert(networkId, load(networkId)).value();
}

QStringList IrcHighlightStore::words(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    return cached(networkId);
}

QStringList IrcHighlightStore::listed(const QString& networkId,
                                      const IrcCaseMapping& mapping) const
{
    QStringList unique;
    for (const QString& word : words(networkId)) {
        if (!listContains(unique, word, mapping))
            unique.append(word);
    }
    return unique;
}

bool IrcHighlightStore::contains(const QString& networkId,
                                 const QString& word,
                                 const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || word.isEmpty())
        return false;
    return listContains(cached(networkId), word, mapping);
}

bool IrcHighlightStore::add(const QString& networkId,
                            const QString& word,
                            const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || word.isEmpty())
        return false;
    QStringList current = cached(networkId);
    if (listContains(current, word, mapping))
        return false;
    current.append(word);
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

bool IrcHighlightStore::remove(const QString& networkId,
                               const QString& word,
                               const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || word.isEmpty())
        return false;
    QStringList current = cached(networkId);
    bool changed = false;
    for (int i = current.size() - 1; i >= 0; --i) {
        if (wordEquals(mapping, current.at(i), word)) {
            current.removeAt(i);
            changed = true;
        }
    }
    if (!changed)
        return false;
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

void IrcHighlightStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}
