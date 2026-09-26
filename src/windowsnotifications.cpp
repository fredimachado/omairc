#include "windowsnotifications.h"

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFileInfo>
#include <QMetaObject>
#include <QStandardPaths>
#include <QStringList>
#include <QUrl>
#include <QUrlQuery>

#ifndef NOMINMAX
#  define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#  define WIN32_LEAN_AND_MEAN
#endif

#include <windows.h>

#include <notificationactivationcallback.h>
#include <propkey.h>
#include <propsys.h>
#include <shobjidl.h>

#include <roapi.h>
#include <windows.data.xml.dom.h>
#include <windows.foundation.h>
#include <windows.ui.notifications.h>

#include <wrl/client.h>
#include <wrl/implements.h>
#include <wrl/wrappers/corewrappers.h>

#include <atomic>
#include <mutex>
#include <thread>
#include <vector>

namespace {

// The single AppUserModelID and toast activator CLSID for the app. Both are
// stamped on the Start Menu shortcut, and the CLSID is written to
// HKCU\SOFTWARE\Classes\CLSID\{...}\LocalServer32 so Action Center can start
// the window from a lingering toast.
constexpr wchar_t kAumid[] = L"FrediMachado.Omairc";
constexpr wchar_t kActivatorClsidText[] = L"{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}";
const CLSID kActivatorClassId = {0xDBE38477, 0x5F87, 0x42A1,
                                 {0xAF, 0xA1, 0x11, 0xFA, 0x12, 0xF2, 0xE5, 0xE5}};

using namespace ABI::Windows::Data::Xml::Dom;
using namespace ABI::Windows::UI::Notifications;
using namespace Microsoft::WRL;
using namespace Microsoft::WRL::Wrappers;

// Set while an instance is alive so the COM activator can reach it from the
// dedicated thread.
std::atomic<WindowsNotifications *> g_notifications{nullptr};

// Serializes a COM activation against teardown. Activate holds this while it
// borrows the live object, and the destructor holds it while it clears the
// pointer, so a click that lands mid-shutdown cannot post to freed memory.
std::mutex g_activationMutex;

bool notificationsAvailable()
{
    if (!qEnvironmentVariableIsEmpty("OMAIRC_SKIP_NOTIFICATIONS"))
        return false;
    if (qgetenv("QT_QPA_PLATFORM") == "offscreen")
        return false;
    // A headless CLI invocation returns from main() before Backend exists, so
    // this is the whole availability policy.
    return true;
}

std::wstring toWide(const QString &value)
{
    return value.toStdWString();
}

// Windows replaces a toast only when the new one repeats both its Tag and its
// Group; toasts that share just a tag stack up in Action Center. Hash the
// conversation into both so a conversation replaces its own toast, matching
// the Linux replacesId and the macOS notification identifier. Sixteen hex
// characters stay well inside the 64-character Tag and Group limit.
QString conversationKey(const QString &networkId, const QString &target)
{
    const QByteArray digest = QCryptographicHash::hash(
        (networkId + QLatin1Char('\n') + target).toUtf8(),
        QCryptographicHash::Sha1);
    return QString::fromLatin1(digest.toHex().left(16));
}

// XML 1.0 content allows #x9, #xA, #xD, #x20-#xD7FF, #xE000-#xFFFD, and
// supplementary code points. Anything else -- the other C0 controls, lone
// surrogates, #xFFFE/#xFFFF -- makes IXmlDocumentIO::LoadXml reject the whole
// document, which silently drops the toast. IRC text reaches this untrusted and
// only has its formatting codes stripped upstream, so filter here rather than
// let one control byte swallow a mention.
QString sanitizeXmlText(const QString &value)
{
    QString sanitized;
    sanitized.reserve(value.size());
    for (int i = 0; i < value.size(); ++i) {
        const ushort code = value.at(i).unicode();
        if (code == 0x09 || code == 0x0A || code == 0x0D
            || (code >= 0x20 && code <= 0xD7FF)
            || (code >= 0xE000 && code <= 0xFFFD)) {
            sanitized += value.at(i);
            continue;
        }
        // A well-formed surrogate pair is a supplementary code point, which
        // XML allows; a lone surrogate is not.
        if (value.at(i).isHighSurrogate() && i + 1 < value.size()
            && value.at(i + 1).isLowSurrogate()) {
            sanitized += value.at(i);
            sanitized += value.at(++i);
        }
    }
    return sanitized;
}

QString escapeXml(const QString &value)
{
    QString escaped = sanitizeXmlText(value);
    escaped.replace(QLatin1Char('&'), QLatin1String("&amp;"));
    escaped.replace(QLatin1Char('<'), QLatin1String("&lt;"));
    escaped.replace(QLatin1Char('>'), QLatin1String("&gt;"));
    escaped.replace(QLatin1Char('"'), QLatin1String("&quot;"));
    escaped.replace(QLatin1Char('\''), QLatin1String("&apos;"));
    escaped.replace(QLatin1Char('\n'), QLatin1String("&#10;"));
    escaped.replace(QLatin1Char('\r'), QLatin1String("&#13;"));
    return escaped;
}

QString buildLaunchArgs(const QString &networkId, const QString &target,
                        const QString &msgid)
{
    QUrl url;
    url.setScheme(QStringLiteral("omairc"));
    url.setHost(QStringLiteral("notify"));
    QUrlQuery query;
    query.addQueryItem(QStringLiteral("network"), networkId);
    query.addQueryItem(QStringLiteral("target"), target);
    query.addQueryItem(QStringLiteral("msgid"), msgid);
    url.setQuery(query);
    return url.toString(QUrl::FullyEncoded);
}

void parseLaunchArgs(const QString &launch, QString *networkId, QString *target,
                     QString *msgid)
{
    const QUrl url = QUrl::fromEncoded(launch.toUtf8());
    const QUrlQuery query(url.query());
    *networkId = query.queryItemValue(QStringLiteral("network"), QUrl::FullyDecoded);
    *target = query.queryItemValue(QStringLiteral("target"), QUrl::FullyDecoded);
    *msgid = query.queryItemValue(QStringLiteral("msgid"), QUrl::FullyDecoded);
}

QString buildToastXml(const QString &summary, const QString &body,
                      const QString &launch)
{
    return QStringLiteral(
               "<toast activationType=\"foreground\" launch=\"%1\">"
               "<visual><binding template=\"ToastGeneric\">"
               "<text>%2</text><text>%3</text>"
               "</binding></visual></toast>")
        .arg(escapeXml(launch), escapeXml(summary), escapeXml(body));
}

// Writes the Start Menu shortcut that carries the AUMID and the toast
// activator CLSID. Without it CreateToastNotifierWithId fails and Action
// Center cannot route a click back to this process.
HRESULT installShortcut(const QString &linkPath, const QString &exePath)
{
    ComPtr<IShellLinkW> shellLink;
    HRESULT hr = CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER,
                                  IID_PPV_ARGS(&shellLink));
    if (FAILED(hr))
        return hr;

    const std::wstring exe = toWide(QDir::toNativeSeparators(exePath));
    hr = shellLink->SetPath(exe.c_str());
    if (FAILED(hr))
        return hr;
    const std::wstring workingDir =
        toWide(QDir::toNativeSeparators(QFileInfo(exePath).absolutePath()));
    shellLink->SetWorkingDirectory(workingDir.c_str());
    shellLink->SetIconLocation(exe.c_str(), 0);

    ComPtr<IPropertyStore> store;
    hr = shellLink.As(&store);
    if (FAILED(hr))
        return hr;

    const std::wstring aumid = kAumid;
    PROPVARIANT id = {};
    id.vt = VT_LPWSTR;
    id.pwszVal = const_cast<wchar_t *>(aumid.c_str());
    hr = store->SetValue(PKEY_AppUserModel_ID, id);
    if (FAILED(hr))
        return hr;

    PROPVARIANT activator = {};
    activator.vt = VT_CLSID;
    activator.puuid = const_cast<CLSID *>(&kActivatorClassId);
    hr = store->SetValue(PKEY_AppUserModel_ToastActivatorCLSID, activator);
    if (FAILED(hr))
        return hr;

    hr = store->Commit();
    if (FAILED(hr))
        return hr;

    ComPtr<IPersistFile> persist;
    hr = shellLink.As(&persist);
    if (FAILED(hr))
        return hr;
    return persist->Save(toWide(QDir::toNativeSeparators(linkPath)).c_str(), TRUE);
}

// The path a shortcut launches, or an empty string when it cannot be read.
QString shortcutTarget(const QString &linkPath)
{
    ComPtr<IShellLinkW> shellLink;
    if (FAILED(CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER,
                                IID_PPV_ARGS(&shellLink))))
        return {};
    ComPtr<IPersistFile> persist;
    if (FAILED(shellLink.As(&persist)))
        return {};
    if (FAILED(persist->Load(toWide(QDir::toNativeSeparators(linkPath)).c_str(),
                             STGM_READ)))
        return {};
    wchar_t target[MAX_PATH] = {};
    if (FAILED(shellLink->GetPath(target, MAX_PATH, nullptr, SLGP_RAWPATH)))
        return {};
    return QDir::fromNativeSeparators(QString::fromWCharArray(target));
}

// True when the shortcut already carries our AUMID and toast activator CLSID,
// so a link the installer stamped can be left untouched. Rewriting one would
// need elevation for an all-users install and would drop its comment.
bool shortcutIsStamped(const QString &linkPath)
{
    ComPtr<IShellLinkW> shellLink;
    if (FAILED(CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER,
                                IID_PPV_ARGS(&shellLink))))
        return false;
    ComPtr<IPersistFile> persist;
    if (FAILED(shellLink.As(&persist)))
        return false;
    if (FAILED(persist->Load(toWide(QDir::toNativeSeparators(linkPath)).c_str(),
                             STGM_READ)))
        return false;
    ComPtr<IPropertyStore> store;
    if (FAILED(shellLink.As(&store)))
        return false;

    PROPVARIANT id = {};
    if (FAILED(store->GetValue(PKEY_AppUserModel_ID, &id)))
        return false;
    const bool aumidMatches =
        id.vt == VT_LPWSTR && id.pwszVal
        && QString::fromWCharArray(id.pwszVal) == QString::fromWCharArray(kAumid);
    PropVariantClear(&id);
    if (!aumidMatches)
        return false;

    PROPVARIANT clsid = {};
    if (FAILED(store->GetValue(PKEY_AppUserModel_ToastActivatorCLSID, &clsid)))
        return false;
    const bool clsidMatches =
        clsid.vt == VT_CLSID && clsid.puuid
        && IsEqualGUID(*clsid.puuid, kActivatorClassId);
    PropVariantClear(&clsid);
    return clsidMatches;
}

// The per-user and all-users Start Menu Programs folders.
QStringList startMenuProgramDirs()
{
    QStringList dirs;
    const QString userPrograms =
        QStandardPaths::writableLocation(QStandardPaths::ApplicationsLocation);
    if (!userPrograms.isEmpty())
        dirs.append(userPrograms);
    const QString programData = qEnvironmentVariable("ProgramData");
    if (!programData.isEmpty())
        dirs.append(programData
                    + QStringLiteral("/Microsoft/Windows/Start Menu/Programs"));
    return dirs;
}

// The installed Start Menu shortcuts that launch this executable.
QStringList installedShortcutPaths(const QString &exePath)
{
    const QString target = QDir::cleanPath(QDir::fromNativeSeparators(exePath));
    QStringList links;
    for (const QString &dir : startMenuProgramDirs()) {
        const QString link = dir + QStringLiteral("/Omairc.lnk");
        if (!QFileInfo::exists(link))
            continue;
        const QString linkTarget = shortcutTarget(link);
        if (linkTarget.isEmpty())
            continue;
        if (QDir::cleanPath(linkTarget).compare(target, Qt::CaseInsensitive) == 0)
            links.append(link);
    }
    return links;
}

// Registers the local COM server so the shell can relaunch this executable to
// deliver a toast activation after the window has closed.
void registerComServer(const QString &exePath)
{
    const QString subKey =
        QStringLiteral("SOFTWARE\\Classes\\CLSID\\%1\\LocalServer32")
            .arg(QString::fromWCharArray(kActivatorClsidText));
    const QString command =
        QLatin1Char('"') + QDir::toNativeSeparators(exePath) + QLatin1Char('"');
    const std::wstring subKeyWide = subKey.toStdWString();
    const std::wstring commandWide = command.toStdWString();
    RegSetKeyValueW(HKEY_CURRENT_USER, subKeyWide.c_str(), nullptr, REG_SZ,
                    commandWide.c_str(),
                    static_cast<DWORD>((commandWide.size() + 1) * sizeof(wchar_t)));
}

void ensureNotificationRegistration()
{
    const QString exePath = QCoreApplication::applicationFilePath();
    registerComServer(exePath);

    // packaging/windows/omairc.iss writes the Start Menu shortcut with the
    // AUMID and the toast activator CLSID already stamped on it. Use that link
    // instead of adding a second "Omairc" entry to the Start Menu, repairing
    // one written before this feature existed when we can. Only a run with no
    // installed shortcut (a portable or development tree) falls through and
    // writes the per-user link itself.
    const QStringList installed = installedShortcutPaths(exePath);
    for (const QString &link : installed) {
        if (shortcutIsStamped(link))
            return;
    }
    for (const QString &link : installed) {
        if (SUCCEEDED(installShortcut(link, exePath)))
            return;
    }

    const QString programs =
        QStandardPaths::writableLocation(QStandardPaths::ApplicationsLocation);
    if (programs.isEmpty())
        return;
    QDir().mkpath(programs);
    installShortcut(programs + QStringLiteral("/Omairc.lnk"), exePath);
}

// The COM activator Windows invokes when the user clicks a toast, whether or
// not the window is already running.
class NotificationActivator
    : public RuntimeClass<RuntimeClassFlags<ClassicCom>,
                          INotificationActivationCallback> {
public:
    HRESULT STDMETHODCALLTYPE Activate(LPCWSTR /*appUserModelId*/,
                                       LPCWSTR invokedArgs,
                                       const NOTIFICATION_USER_INPUT_DATA * /*data*/,
                                       ULONG /*count*/) override
    {
        // Held across QMetaObject::invokeMethod so teardown cannot free the
        // object between the pointer load and the post.
        std::lock_guard<std::mutex> lock(g_activationMutex);
        WindowsNotifications *owner = g_notifications.load();
        if (!owner || !invokedArgs)
            return S_OK;
        QString networkId;
        QString target;
        QString msgid;
        parseLaunchArgs(QString::fromWCharArray(invokedArgs), &networkId, &target,
                        &msgid);
        // Match the macOS delegate: a launch string that names no conversation
        // must not reach Backend::notificationActivated and the window.
        if (networkId.isEmpty() || target.isEmpty())
            return S_OK;
        QMetaObject::invokeMethod(
            owner,
            [owner, networkId, target, msgid]() {
                emit owner->activated(networkId, target, msgid);
            },
            Qt::QueuedConnection);
        return S_OK;
    }
};

class ActivatorFactory
    : public RuntimeClass<RuntimeClassFlags<ClassicCom>, IClassFactory> {
public:
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown *outer, REFIID riid,
                                             void **ppv) override
    {
        if (ppv)
            *ppv = nullptr;
        if (outer)
            return CLASS_E_NOAGGREGATION;
        ComPtr<NotificationActivator> activator = Make<NotificationActivator>();
        if (!activator)
            return E_OUTOFMEMORY;
        return activator->QueryInterface(riid, ppv);
    }

    HRESULT STDMETHODCALLTYPE LockServer(BOOL lock) override
    {
        if (lock)
            CoAddRefServerProcess();
        else
            CoReleaseServerProcess();
        return S_OK;
    }
};

void showToast(const QString &summary, const QString &body,
               const QString &networkId, const QString &target,
               const QString &msgid)
{
    ComPtr<IToastNotificationManagerStatics> manager;
    if (FAILED(Windows::Foundation::GetActivationFactory(
            HStringReference(RuntimeClass_Windows_UI_Notifications_ToastNotificationManager)
                .Get(),
            &manager)))
        return;

    ComPtr<IToastNotifier> notifier;
    if (FAILED(manager->CreateToastNotifierWithId(HStringReference(kAumid).Get(),
                                                  &notifier)))
        return;

    ComPtr<IXmlDocument> document;
    if (FAILED(Windows::Foundation::ActivateInstance(
            HStringReference(RuntimeClass_Windows_Data_Xml_Dom_XmlDocument).Get(),
            &document)))
        return;
    ComPtr<IXmlDocumentIO> documentIo;
    if (FAILED(document.As(&documentIo)))
        return;

    const std::wstring xml =
        buildToastXml(summary, body, buildLaunchArgs(networkId, target, msgid))
            .toStdWString();
    if (FAILED(documentIo->LoadXml(HStringReference(xml.c_str()).Get())))
        return;

    ComPtr<IToastNotificationFactory> factory;
    if (FAILED(Windows::Foundation::GetActivationFactory(
            HStringReference(RuntimeClass_Windows_UI_Notifications_ToastNotification)
                .Get(),
            &factory)))
        return;

    ComPtr<IToastNotification> notification;
    if (FAILED(factory->CreateToastNotification(document.Get(), &notification)))
        return;

    // put_Tag and put_Group live on IToastNotification2, not IToastNotification.
    // Setting both to the conversation key is what makes a later toast replace
    // the earlier one instead of stacking beside it.
    const std::wstring key = conversationKey(networkId, target).toStdWString();
    ComPtr<IToastNotification2> notification2;
    if (SUCCEEDED(notification.As(&notification2))) {
        notification2->put_Tag(HStringReference(key.c_str()).Get());
        notification2->put_Group(HStringReference(key.c_str()).Get());
    }
    notifier->Show(notification.Get());
}

} // namespace

struct WindowsNotificationsImpl {
    struct Toast {
        QString summary;
        QString body;
        QString networkId;
        QString target;
        QString msgid;
    };

    std::thread thread;
    // Wake the notification thread for queued work, and for shutdown. Events
    // replace PostThreadMessage so nothing races the thread's message queue.
    HANDLE stopEvent = nullptr;
    HANDLE toastEvent = nullptr;
    DWORD cookie = 0;
    bool comServerAddRef = false;
    // Published by the notification thread once the COM activator is
    // registered, so delivery only starts when a toast can actually render.
    // Atomic because notify() reads it on the Qt main thread.
    std::atomic<bool> enabled{false};
    std::mutex mutex;
    std::vector<Toast> queue;
};

namespace {

void runNotificationsThread(WindowsNotificationsImpl *impl)
{
    const HRESULT roHr = RoInitialize(RO_INIT_MULTITHREADED);

    // Registration, the registry write, and the Start Menu shortcut all happen
    // here rather than in the constructor, so opening the window never blocks
    // the GUI thread on COM, the registry, or the filesystem. Delivery stays
    // off until the class object is registered.
    if (SUCCEEDED(roHr)) {
        ensureNotificationRegistration();
        ComPtr<ActivatorFactory> factory = Make<ActivatorFactory>();
        if (factory) {
            if (SUCCEEDED(CoRegisterClassObject(kActivatorClassId, factory.Get(),
                                                CLSCTX_LOCAL_SERVER,
                                                REGCLS_MULTIPLEUSE, &impl->cookie))) {
                CoAddRefServerProcess();
                impl->comServerAddRef = true;
                impl->enabled.store(true, std::memory_order_release);
            }
        }
    }

    const HANDLE handles[] = {impl->stopEvent, impl->toastEvent};
    for (;;) {
        const DWORD wait =
            MsgWaitForMultipleObjects(2, handles, FALSE, INFINITE, QS_ALLINPUT);
        if (wait == WAIT_OBJECT_0)
            break;
        if (wait == WAIT_OBJECT_0 + 1) {
            std::vector<WindowsNotificationsImpl::Toast> pending;
            {
                std::lock_guard<std::mutex> lock(impl->mutex);
                pending.swap(impl->queue);
            }
            for (const auto &toast : pending)
                showToast(toast.summary, toast.body, toast.networkId,
                          toast.target, toast.msgid);
            continue;
        }
        if (wait == WAIT_OBJECT_0 + 2) {
            // COM activation traffic. Pump it so an activation that arrives on
            // this thread is dispatched instead of stalling the server.
            MSG msg;
            while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            continue;
        }
        break; // WAIT_FAILED or WAIT_ABANDONED
    }

    if (impl->cookie)
        CoRevokeClassObject(impl->cookie);
    if (impl->comServerAddRef)
        CoReleaseServerProcess();
    if (SUCCEEDED(roHr))
        RoUninitialize();
}

} // namespace

WindowsNotifications::WindowsNotifications(QObject *parent) : QObject(parent)
{
    if (!notificationsAvailable())
        return;
    m_impl = new WindowsNotificationsImpl;
    m_impl->stopEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    m_impl->toastEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    // Without the events there is no way to wake or stop the thread, so a
    // failed registration could not be reported. Do without toasts instead.
    if (!m_impl->stopEvent || !m_impl->toastEvent) {
        if (m_impl->stopEvent)
            CloseHandle(m_impl->stopEvent);
        if (m_impl->toastEvent)
            CloseHandle(m_impl->toastEvent);
        delete m_impl;
        m_impl = nullptr;
        return;
    }
    g_notifications.store(this);
    // Do not wait for registration here. The notification thread enables
    // delivery once the COM activator is registered; until then notify() is a
    // no-op, which is correct for the first moments of startup.
    m_impl->thread = std::thread(runNotificationsThread, m_impl);
}

WindowsNotifications::~WindowsNotifications()
{
    if (!m_impl)
        return;
    // Retire the object from COM before joining, so an activation that is
    // already in flight finishes while the object is still alive and any later
    // one finds no owner.
    {
        std::lock_guard<std::mutex> lock(g_activationMutex);
        g_notifications.store(nullptr);
    }
    SetEvent(m_impl->stopEvent);
    if (m_impl->thread.joinable())
        m_impl->thread.join();
    CloseHandle(m_impl->stopEvent);
    CloseHandle(m_impl->toastEvent);
    delete m_impl;
    m_impl = nullptr;
}

void WindowsNotifications::notify(const QString &summary, const QString &body,
                                  const QString &networkId, const QString &target,
                                  const QString &msgid)
{
    if (!m_impl || !m_impl->enabled.load(std::memory_order_acquire))
        return;
    {
        std::lock_guard<std::mutex> lock(m_impl->mutex);
        m_impl->queue.push_back({summary, body, networkId, target, msgid});
    }
    SetEvent(m_impl->toastEvent);
}
