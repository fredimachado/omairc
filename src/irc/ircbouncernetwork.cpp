#include "ircbouncernetwork.h"

#include "ircwiretext.h"

#include <QList>
#include <QStringList>

namespace
{
QString fromWire(std::string_view bytes)
{
    return ircWireText(bytes);
}

bool equalsFolded(QStringView left, QLatin1String right)
{
    return left.compare(right, Qt::CaseInsensitive) == 0;
}

bool knownKey(const QString& key)
{
    return equalsFolded(key, QLatin1String("name"))
        || equalsFolded(key, QLatin1String("host"))
        || equalsFolded(key, QLatin1String("port"))
        || equalsFolded(key, QLatin1String("tls"))
        || equalsFolded(key, QLatin1String("nickname"))
        || equalsFolded(key, QLatin1String("username"))
        || equalsFolded(key, QLatin1String("realname"))
        || equalsFolded(key, QLatin1String("pass"))
        || equalsFolded(key, QLatin1String("state"))
        || equalsFolded(key, QLatin1String("error"));
}

int defaultPort(bool tls)
{
    return tls ? 6697 : 6667;
}

void recomputePort(IrcBouncerNetwork& network)
{
    if (!network.portExplicit)
        network.port = defaultPort(network.tls);
}

IrcBouncerNetwork freshNetwork(const QString& id)
{
    IrcBouncerNetwork network;
    network.id = id;
    network.tls = true;
    network.port = defaultPort(true);
    return network;
}

void applyAttribute(IrcBouncerNetwork& network, const IrcBouncerAttribute& attribute)
{
    const QString key = fromWire(attribute.key);
    if (!knownKey(key))
        return;
    const QString value = fromWire(attribute.value);
    if (equalsFolded(key, QLatin1String("name"))) {
        network.name = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("host"))) {
        network.host = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("nickname"))) {
        network.nickname = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("username"))) {
        network.username = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("realname"))) {
        network.realname = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("pass"))) {
        network.pass = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("error"))) {
        network.error = attribute.removed ? QString() : value;
        return;
    }
    if (equalsFolded(key, QLatin1String("state"))) {
        if (attribute.removed) {
            network.state = IrcBouncerNetworkState::Disconnected;
            network.stateSet = false;
            return;
        }
        if (equalsFolded(value, QLatin1String("connected"))) {
            network.state = IrcBouncerNetworkState::Connected;
            network.stateSet = true;
        } else if (equalsFolded(value, QLatin1String("connecting"))) {
            network.state = IrcBouncerNetworkState::Connecting;
            network.stateSet = true;
        } else if (equalsFolded(value, QLatin1String("disconnected"))) {
            network.state = IrcBouncerNetworkState::Disconnected;
            network.stateSet = true;
        }
        return;
    }
    if (equalsFolded(key, QLatin1String("tls"))) {
        if (attribute.removed) {
            network.tls = true;
            network.tlsExplicit = false;
            recomputePort(network);
            return;
        }
        if (value == QLatin1String("1") || value == QLatin1String("0")) {
            network.tls = value == QLatin1String("1");
            network.tlsExplicit = true;
            recomputePort(network);
        }
        return;
    }
    if (equalsFolded(key, QLatin1String("port"))) {
        if (attribute.removed) {
            network.portExplicit = false;
            recomputePort(network);
            return;
        }
        bool ok = false;
        const int port = value.toInt(&ok);
        if (ok && port > 0 && port <= 65535) {
            network.port = port;
            network.portExplicit = true;
        }
    }
}

void applyAttributes(IrcBouncerNetwork& network,
                     const std::vector<IrcBouncerAttribute>& attributes)
{
    for (const IrcBouncerAttribute& attribute : attributes)
        applyAttribute(network, attribute);
}

int indexOf(const QList<IrcBouncerNetwork>& networks, const QString& id)
{
    for (int index = 0; index < networks.size(); ++index) {
        if (networks.at(index).id == id)
            return index;
    }
    return -1;
}

bool wireSafe(const QString& text)
{
    return !text.contains(QLatin1Char('\r'))
        && !text.contains(QLatin1Char('\n'))
        && !text.contains(QChar(0));
}

bool validNetId(const QString& netId)
{
    if (netId.isEmpty() || !wireSafe(netId))
        return false;
    return !netId.contains(QLatin1Char(' '))
        && !netId.contains(QLatin1Char(':'));
}

bool validAttributeKey(const QString& key)
{
    if (key.isEmpty() || !wireSafe(key))
        return false;
    for (const QChar ch : key) {
        const bool ok = (ch >= QLatin1Char('A') && ch <= QLatin1Char('Z'))
            || (ch >= QLatin1Char('a') && ch <= QLatin1Char('z'))
            || (ch >= QLatin1Char('0') && ch <= QLatin1Char('9'))
            || ch == QLatin1Char('-');
        if (!ok)
            return false;
    }
    return true;
}

QString escapeTagValue(const QString& value)
{
    QString escaped;
    escaped.reserve(value.size());
    for (const QChar ch : value) {
        if (ch == QLatin1Char('\\'))
            escaped += QLatin1String("\\\\");
        else if (ch == QLatin1Char(';'))
            escaped += QLatin1String("\\:");
        else if (ch == QLatin1Char(' '))
            escaped += QLatin1String("\\s");
        else if (ch == QLatin1Char('\r'))
            escaped += QLatin1String("\\r");
        else if (ch == QLatin1Char('\n'))
            escaped += QLatin1String("\\n");
        else
            escaped += ch;
    }
    return escaped;
}

std::optional<QString> encodeAttributes(
    const QList<IrcBouncerAttributeValue>& attributes)
{
    if (attributes.isEmpty())
        return std::nullopt;
    QStringList pieces;
    for (const IrcBouncerAttributeValue& attribute : attributes) {
        if (!validAttributeKey(attribute.key) || !wireSafe(attribute.value))
            return std::nullopt;
        if (attribute.value.isEmpty())
            pieces.append(attribute.key + QLatin1Char('='));
        else
            pieces.append(attribute.key + QLatin1Char('=') + escapeTagValue(attribute.value));
    }
    return pieces.join(QLatin1Char(';'));
}

std::optional<IrcBouncerFailure::Code> failureCode(const QString& name)
{
    if (equalsFolded(name, QLatin1String("UNKNOWN_COMMAND")))
        return IrcBouncerFailure::Code::UnknownCommand;
    if (equalsFolded(name, QLatin1String("ACCOUNT_REQUIRED")))
        return IrcBouncerFailure::Code::AccountRequired;
    if (equalsFolded(name, QLatin1String("REGISTRATION_IS_COMPLETED")))
        return IrcBouncerFailure::Code::RegistrationIsCompleted;
    if (equalsFolded(name, QLatin1String("INVALID_NETID")))
        return IrcBouncerFailure::Code::InvalidNetId;
    if (equalsFolded(name, QLatin1String("INVALID_ATTRIBUTE")))
        return IrcBouncerFailure::Code::InvalidAttribute;
    if (equalsFolded(name, QLatin1String("READ_ONLY_ATTRIBUTE")))
        return IrcBouncerFailure::Code::ReadOnlyAttribute;
    if (equalsFolded(name, QLatin1String("UNKNOWN_ATTRIBUTE")))
        return IrcBouncerFailure::Code::UnknownAttribute;
    if (equalsFolded(name, QLatin1String("NEED_ATTRIBUTE")))
        return IrcBouncerFailure::Code::NeedAttribute;
    return std::nullopt;
}

bool attributeFailure(IrcBouncerFailure::Code code)
{
    return code == IrcBouncerFailure::Code::InvalidAttribute
        || code == IrcBouncerFailure::Code::ReadOnlyAttribute
        || code == IrcBouncerFailure::Code::UnknownAttribute
        || code == IrcBouncerFailure::Code::NeedAttribute;
}
}

void IrcBouncerNetworkTable::clear()
{
    m_networks.clear();
}

void IrcBouncerNetworkTable::apply(const IrcParsedBouncer& command)
{
    if (command.subcommand != IrcBouncerSubcommand::Network)
        return;
    const QString id = fromWire(command.netId);
    if (id.isEmpty())
        return;
    if (command.removed) {
        const int found = indexOf(m_networks, id);
        if (found >= 0)
            m_networks.removeAt(found);
        return;
    }
    int found = indexOf(m_networks, id);
    if (found < 0) {
        m_networks.append(freshNetwork(id));
        found = m_networks.size() - 1;
    }
    applyAttributes(m_networks[found], command.attributes);
}

void IrcBouncerNetworkTable::replaceSnapshot(const QList<IrcParsedBouncer>& lines)
{
    QList<IrcBouncerNetwork> next;
    for (const IrcParsedBouncer& line : lines) {
        if (line.subcommand != IrcBouncerSubcommand::Network)
            continue;
        const QString id = fromWire(line.netId);
        if (id.isEmpty())
            continue;
        if (line.removed) {
            const int found = indexOf(next, id);
            if (found >= 0)
                next.removeAt(found);
            continue;
        }
        int found = indexOf(next, id);
        if (found < 0) {
            next.append(freshNetwork(id));
            found = next.size() - 1;
        }
        applyAttributes(next[found], line.attributes);
    }
    m_networks = next;
}

QList<IrcBouncerNetwork> IrcBouncerNetworkTable::list() const
{
    return m_networks;
}

std::optional<IrcBouncerFailure> parseBouncerFailure(const IrcMessage& message)
{
    if (!equalsFolded(fromWire(message.command), QLatin1String("FAIL")))
        return std::nullopt;
    if (message.parameters.size() < 4)
        return std::nullopt;
    if (!equalsFolded(fromWire(message.parameters[0]), QLatin1String("BOUNCER")))
        return std::nullopt;
    const QString codeName = fromWire(message.parameters[1]);
    const std::optional<IrcBouncerFailure::Code> code = failureCode(codeName);
    if (!code)
        return std::nullopt;

    IrcBouncerFailure failure;
    failure.code = *code;
    failure.codeName = codeName;
    failure.subcommand = fromWire(message.parameters[2]);
    failure.description = fromWire(message.parameters.back());
    QStringList context;
    for (std::size_t index = 3; index + 1 < message.parameters.size(); ++index)
        context.append(fromWire(message.parameters[index]));
    if (*code == IrcBouncerFailure::Code::InvalidNetId) {
        if (!context.isEmpty())
            failure.netId = context.first();
    } else if (attributeFailure(*code)) {
        if (context.size() >= 2) {
            failure.netId = context.at(0);
            failure.attribute = context.at(1);
        } else if (context.size() == 1) {
            failure.attribute = context.first();
        }
    }
    return failure;
}

QString ircBouncerListCommand()
{
    return QStringLiteral("BOUNCER LISTNETWORKS");
}

std::optional<QString> ircBouncerAddCommand(
    const QList<IrcBouncerAttributeValue>& attributes)
{
    const std::optional<QString> encoded = encodeAttributes(attributes);
    if (!encoded)
        return std::nullopt;
    return QStringLiteral("BOUNCER ADDNETWORK ") + *encoded;
}

std::optional<QString> ircBouncerChangeCommand(
    const QString& netId, const QList<IrcBouncerAttributeValue>& attributes)
{
    if (!validNetId(netId))
        return std::nullopt;
    const std::optional<QString> encoded = encodeAttributes(attributes);
    if (!encoded)
        return std::nullopt;
    return QStringLiteral("BOUNCER CHANGENETWORK ") + netId + QLatin1Char(' ') + *encoded;
}

std::optional<QString> ircBouncerDeleteCommand(const QString& netId)
{
    if (!validNetId(netId))
        return std::nullopt;
    return QStringLiteral("BOUNCER DELNETWORK ") + netId;
}

std::optional<QString> ircBouncerBindCommand(const QString& netId)
{
    if (!validNetId(netId))
        return std::nullopt;
    return QStringLiteral("BOUNCER BIND ") + netId;
}
