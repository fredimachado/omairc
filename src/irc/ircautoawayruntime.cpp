#include "ircautoawayruntime.h"

#include "ircprofilestore.h"
#include "ircsession.h"

#include <QCoreApplication>
#include <QEvent>
#include <QFileInfo>
#ifdef QT_GUI_LIB
#include <QGuiApplication>
#endif
#include <QSettings>

namespace {

QString autoawayEnabledKey()
{
    return QStringLiteral("autoawayEnabled");
}

QString autoawayTimeoutSecondsKey()
{
    return QStringLiteral("autoawayTimeoutSeconds");
}

QString autoawayDefaultReasonKey()
{
    return QStringLiteral("autoawayDefaultReason");
}

IrcAutoawayConfig loadAutoaway()
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    IrcAutoawayConfig config;
    config.enabled = settings.value(autoawayEnabledKey(), false).toBool();
    config.timeoutSeconds =
        settings.value(autoawayTimeoutSecondsKey(), 0).toInt();
    if (config.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        config.timeoutSeconds = 0;
    if (config.timeoutSeconds > ircAutoawayMaxTimeoutSeconds)
        config.timeoutSeconds = ircAutoawayMaxTimeoutSeconds;
    if (config.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        config.enabled = false;
    config.defaultReason = settings.value(autoawayDefaultReasonKey()).toString();
    return config;
}

bool preferenceIniRefusesWrite(const QSettings &settings)
{
    if (IrcProfileStore::probeSettingsIni(settings) != IrcProfileStore::IniProbe::Ok)
        return true;
    const QFileInfo info(settings.fileName());
    return info.exists() && !info.isWritable();
}

void saveAutoawayConfig(const IrcAutoawayConfig& config)
{
    QSettings settings;
    if (preferenceIniRefusesWrite(settings))
        return;
    settings.beginGroup(QStringLiteral("preferences"));
    settings.setValue(autoawayEnabledKey(), config.enabled);
    settings.setValue(autoawayTimeoutSecondsKey(), config.timeoutSeconds);
    settings.setValue(autoawayDefaultReasonKey(), config.defaultReason);
    settings.endGroup();
    settings.sync();
}

} // namespace

IrcAutoawayRuntime::IrcAutoawayRuntime(Host host)
    : m_host(std::move(host))
{
    m_autoawayIdle.setSingleShot(true);
    m_autoawayGrace.setSingleShot(true);
    connect(&m_autoawayIdle, &QTimer::timeout, this,
            &IrcAutoawayRuntime::onAutoawayIdle);
    connect(&m_autoawayGrace, &QTimer::timeout, this,
            &IrcAutoawayRuntime::onAutoawayGrace);
    if (QCoreApplication *app = QCoreApplication::instance())
        app->installEventFilter(this);
}

IrcAutoawayRuntime::~IrcAutoawayRuntime()
{
    if (QCoreApplication *app = QCoreApplication::instance())
        app->removeEventFilter(this);
}

void IrcAutoawayRuntime::setEphemeral(bool ephemeral)
{
    m_ephemeral = ephemeral;
}

void IrcAutoawayRuntime::loadStored()
{
    m_autoaway = loadAutoaway();
    armAutoawayIdle();
}

IrcCommandOutcome IrcAutoawayRuntime::dispatchAutoaway(const IrcCommand& command,
                                                       IrcComposerSurface surface)
{
    const IrcAutoawayRequest request = ircParseAutoawayArgument(command.argument);
    if (request.kind == IrcAutoawayKind::Usage)
        return echoAutoawayUsage(surface);

    bool persist = false;
    QString text;
    const QString previousReason = autoawayReason();
    switch (request.kind) {
    case IrcAutoawayKind::Query:
        text = ircFormatAutoawayQuery(m_autoaway);
        break;
    case IrcAutoawayKind::Disable:
        m_autoaway.enabled = false;
        persist = true;
        stopAutoawayTimers();
        m_autoawayTripped = false;
        clearAutoAwayNetworks(false);
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::EnableOn:
        if (m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
            return echoAutoawayUsage(surface);
        m_autoaway.enabled = true;
        persist = true;
        if (!m_autoawayTripped)
            armAutoawayIdle();
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::SetTimeout:
        m_autoaway.enabled = true;
        m_autoaway.timeoutSeconds = request.timeoutSeconds;
        if (!request.text.isEmpty())
            m_autoaway.oneShotReason = request.text;
        persist = true;
        if (!m_autoawayTripped)
            armAutoawayIdle();
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::SetDefaultReason:
        m_autoaway.defaultReason = request.text;
        persist = true;
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::ClearDefaultReason:
        m_autoaway.defaultReason.clear();
        persist = true;
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::Usage:
        return echoAutoawayUsage(surface);
    }
    if (persist)
        saveAutoaway();
    if (m_autoawayTripped && m_autoaway.enabled
        && autoawayReason() != previousReason) {
        refreshAutoAwayReason();
    }
    return echoAutoawayFeedback(surface, text);
}

IrcCommandOutcome IrcAutoawayRuntime::echoAutoawayFeedback(
    IrcComposerSurface surface, const QString& text)
{
    QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty())
        networkId = m_host.consoleNetworkId();
    if (networkId.isEmpty())
        return IrcCommandOutcome::Refused;

    if (surface == IrcComposerSurface::Conversation) {
        const std::optional<IrcConversationKey> selected = m_host.selectedKey();
        if (!selected)
            return IrcCommandOutcome::WrongScope;
        m_host.applyEvent(IrcWhoisTranscriptEvent{*selected, text});
        return IrcCommandOutcome::Sent;
    }
    m_host.recordStatus(networkId, text);
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcAutoawayRuntime::echoAutoawayUsage(IrcComposerSurface surface)
{
    const IrcVerbSpec *spec = IrcVerbTable::find(IrcCommand::Verb::Autoaway);
    return echoAutoawayFeedback(
        surface,
        spec ? spec->usage
             : QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]"));
}

void IrcAutoawayRuntime::saveAutoaway() const
{
    if (m_ephemeral)
        return;
    saveAutoawayConfig(m_autoaway);
}

void IrcAutoawayRuntime::armAutoawayIdle()
{
    stopAutoawayTimers();
    if (!m_autoaway.enabled
        || m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds
        || m_autoaway.timeoutSeconds > ircAutoawayMaxTimeoutSeconds) {
        return;
    }
    m_autoawayIdle.start(m_autoaway.timeoutSeconds * 1000);
}

void IrcAutoawayRuntime::stopAutoawayTimers()
{
    m_autoawayIdle.stop();
    m_autoawayGrace.stop();
    m_autoawayGraceArmed = false;
}

void IrcAutoawayRuntime::onAutoawayIdle()
{
    if (!m_autoaway.enabled
        || m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        return;
    m_autoawayIdle.stop();
    m_autoawayGraceArmed = true;
    const int graceMs = ircAutoawayGraceSeconds(m_autoaway.timeoutSeconds) * 1000;
    if (graceMs <= 0) {
        onAutoawayGrace();
        return;
    }
    m_autoawayGrace.start(graceMs);
}

void IrcAutoawayRuntime::onAutoawayGrace()
{
    if (!m_autoawayGraceArmed)
        return;
    m_autoawayGraceArmed = false;
    m_autoawayGrace.stop();
    tripAutoaway();
}

QString IrcAutoawayRuntime::autoawayReason() const
{
    return !m_autoaway.oneShotReason.isEmpty()
        ? m_autoaway.oneShotReason
        : m_autoaway.defaultReason;
}

void IrcAutoawayRuntime::recordAutoawayStatus(const QString& networkId,
                                              const QString& text)
{
    if (networkId.isEmpty() || text.isEmpty())
        return;
    m_host.recordStatus(networkId, text);
}

bool IrcAutoawayRuntime::markSessionAutoAway(IrcSession *session)
{
    if (!session || session->state() != IrcSession::State::Registered)
        return false;
    if (!session->markAway(autoawayReason()))
        return false;
    m_autoAwayNetworks.insert(session->networkId());
    return true;
}

void IrcAutoawayRuntime::noteManualAway(const QString& networkId)
{
    m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.insert(networkId);
}

void IrcAutoawayRuntime::noteAwayCleared(const QString& networkId)
{
    const bool wasAuto = m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.remove(networkId);
    if (wasAuto && m_autoAwayNetworks.isEmpty())
        m_autoaway.oneShotReason.clear();
}

void IrcAutoawayRuntime::onSessionRegistered(IrcSession *session)
{
    if (m_autoawayTripped && m_autoaway.enabled)
        markSessionAutoAway(session);
}

void IrcAutoawayRuntime::forgetNetwork(const QString& networkId)
{
    m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.remove(networkId);
}

void IrcAutoawayRuntime::refreshAutoAwayReason()
{
    if (!m_autoawayTripped || !m_autoaway.enabled)
        return;
    const QString reason = autoawayReason();
    for (const QString& networkId : m_autoAwayNetworks) {
        IrcSession *session = m_host.findSession(networkId);
        if (!session || session->state() != IrcSession::State::Registered)
            continue;
        session->markAway(reason);
    }
}

void IrcAutoawayRuntime::tripAutoaway()
{
    if (!m_autoaway.enabled)
        return;
    m_autoawayTripped = true;
    const QString status = ircFormatAutoawayTrippedStatus(autoawayReason());
    for (const QString& networkId : m_host.networkIds()) {
        if (m_autoAwayNetworks.contains(networkId)
            || m_manualAwayNetworks.contains(networkId)
            || m_host.selfAway(networkId)) {
            continue;
        }
        if (markSessionAutoAway(m_host.findSession(networkId)))
            recordAutoawayStatus(networkId, status);
    }
    stopAutoawayTimers();
}

void IrcAutoawayRuntime::clearAutoAwayNetworks(bool logCleared)
{
    m_autoawayTripped = false;
    const QSet<QString> networks = m_autoAwayNetworks;
    m_autoAwayNetworks.clear();
    const QString status = logCleared ? ircFormatAutoawayClearedStatus() : QString{};
    for (const QString& networkId : networks) {
        if (IrcSession *session = m_host.findSession(networkId)) {
            if (session->state() == IrcSession::State::Registered
                && session->clearAway()) {
                m_host.markUnawaySent(networkId);
                if (logCleared)
                    recordAutoawayStatus(networkId, status);
            }
        }
    }
    m_autoaway.oneShotReason.clear();
}

void IrcAutoawayRuntime::noteLocalActivity()
{
    const bool wasTripped = m_autoawayTripped;
    m_autoawayTripped = false;
    if (!m_autoAwayNetworks.isEmpty())
        clearAutoAwayNetworks();
    else if (wasTripped)
        m_autoaway.oneShotReason.clear();
    if (m_autoaway.enabled)
        armAutoawayIdle();
}

#ifdef OMAIRC_TEST
void IrcAutoawayRuntime::fireAutoawayIdleForTest()
{
    onAutoawayIdle();
}

void IrcAutoawayRuntime::fireAutoawayGraceForTest()
{
    onAutoawayGrace();
}

int IrcAutoawayRuntime::autoawayIdleIntervalMsForTest() const
{
    return m_autoawayIdle.interval();
}

bool IrcAutoawayRuntime::autoawayIdleIsActiveForTest() const
{
    return m_autoawayIdle.isActive();
}

int IrcAutoawayRuntime::autoawayGraceIntervalMsForTest() const
{
    return m_autoawayGrace.interval();
}

bool IrcAutoawayRuntime::autoawayGraceIsActiveForTest() const
{
    return m_autoawayGrace.isActive();
}
#endif

bool IrcAutoawayRuntime::eventFilter(QObject *watched, QEvent *event)
{
    Q_UNUSED(watched);
    if (!m_autoaway.enabled && !m_autoawayTripped && m_autoAwayNetworks.isEmpty())
        return false;
    switch (event->type()) {
    case QEvent::KeyPress:
    case QEvent::MouseButtonPress:
    case QEvent::TouchBegin:
    case QEvent::TabletPress:
    case QEvent::Wheel:
        break;
    default:
        return false;
    }
#ifdef QT_GUI_LIB
    if (const auto *gui =
            qobject_cast<QGuiApplication *>(QCoreApplication::instance())) {
        if (gui->applicationState() != Qt::ApplicationActive)
            return false;
    }
#endif
    noteLocalActivity();
    return false;
}
