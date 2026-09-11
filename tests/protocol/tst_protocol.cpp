/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * IRCClient is free software; you can redistribute it and/or modify it under
 * the terms of the GNU Lesser General Public License as published by the Free
 * Software Foundation; either version 3.0 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public License
 * for more details.
 *
 * https://www.gnu.org/licenses/lgpl.html
 *
 * Omairc adapted the protocol behavior and tests from IRCClient.
 */

#include <QDate>
#include <QDateTime>
#include <QString>
#include <QTest>
#include <QTime>
#include <QTimeZone>

#include "irccommandbuilder.h"
#include "ircevent.h"
#include "irceventtranslator.h"
#include "ircframer.h"
#include "ircparser.h"
#include "ircserverfeatures.h"
#include "ircwiretext.h"

#include <string>
#include <string_view>
#include <variant>
#include <vector>

namespace
{
QString text(const std::string& value)
{
    return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
}

std::vector<IrcEvent> translate(const IrcMessage& message)
{
    const IrcServerFeatures features;
    return IrcEventTranslator::translate(
        QStringLiteral("net"), QStringLiteral("me"), features, message);
}

QDateTime exampleServerTime()
{
    return QDateTime(QDate(2011, 10, 19), QTime(16, 40, 51, 620), QTimeZone::UTC);
}

bool isNearCurrentUtc(const QDateTime& value)
{
    if (!value.isValid())
        return false;
    return qAbs(value.toUTC().msecsTo(QDateTime::currentDateTimeUtc())) < 5000;
}
}

class ProtocolTest : public QObject
{
    Q_OBJECT

private slots:
    void parsesTrailingParameters();
    void preservesMiddleParameters();
    void limitsParameters();
    void rejectsMalformedMessages();
    void classifiesPrefixes();
    void parsesTags();
    void parsesClientOnlyTypingTag();
    void preservesUtf8();
    void decodesValidUtf8WireText();
    void decodesInvalidUtf8AsLatin1();
    void framesLatin1ThenUtf8();
    void framesFragmentedAndCoalescedInput();
    void rejectsNulAndRecovers();
    void rejectsOverlongAndRecovers();
    void enforcesClassicFrameBoundary();
    void acceptsTaggedClassicFrame();
    void enforcesTagSectionBoundary();
    void buildsRegistration();
    void rejectsInvalidRegistration();
    void rejectsOutboundInjection();
    void enforcesOutboundBoundary();
    void privmsgUsesIrcv3TimeTag();
    void privmsgWithoutTimeUsesCurrentUtc();
    void privmsgInvalidTimeUsesCurrentUtc();
    void actionAndTypingUseIrcv3TimeTag();
    void buildsJoin();
    void rejectsInvalidJoin();
};

void ProtocolTest::parsesTrailingParameters()
{
    auto message = IrcParser::parse(":n!u@h PRIVMSG #c :hello world");
    QVERIFY(message);
    QCOMPARE(message.value->parameters.size(), std::size_t(2));
    QCOMPARE(text(message.value->parameters[0]), QStringLiteral("#c"));
    QCOMPARE(text(message.value->parameters[1]), QStringLiteral("hello world"));

    message = IrcParser::parse(":n!u@h PRIVMSG #c :");
    QVERIFY(message);
    QCOMPARE(message.value->parameters.size(), std::size_t(2));
    QVERIFY(message.value->parameters[1].empty());
}

void ProtocolTest::preservesMiddleParameters()
{
    const auto message = IrcParser::parse("PRIVMSG #channel hello");
    QVERIFY(message);
    QCOMPARE(message.value->parameters.size(), std::size_t(2));
    QCOMPARE(text(message.value->parameters[0]), QStringLiteral("#channel"));
    QCOMPARE(text(message.value->parameters[1]), QStringLiteral("hello"));
}

void ProtocolTest::limitsParameters()
{
    const auto message = IrcParser::parse("TEST 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17");
    QVERIFY(message);
    QCOMPARE(message.value->parameters.size(), std::size_t(15));
    QCOMPARE(text(message.value->parameters[13]), QStringLiteral("14"));
    QCOMPARE(text(message.value->parameters[14]), QStringLiteral("15 16 17"));
}

void ProtocolTest::rejectsMalformedMessages()
{
    const std::vector<std::string> malformed = {
        ":nick! PRIVMSG #c :hi",
        ":nick@ PRIVMSG #c :hi",
        ":@host PRIVMSG #c :hi",
        ":nospacePRIVMSG",
        ": ",
        "!!!",
        "22",
        "2222",
        "@tag=x",
        "@=value PRIVMSG #c :hi",
        "PR1VMSG #c :hi",
    };

    for (const auto& line : malformed)
        QVERIFY2(!IrcParser::parse(line), line.c_str());
}

void ProtocolTest::classifiesPrefixes()
{
    auto message = IrcParser::parse(":alice PRIVMSG #room :hello");
    QVERIFY(message);
    QVERIFY(message.value->prefix.has_value());
    QCOMPARE(text(message.value->prefix->raw), QStringLiteral("alice"));
    QVERIFY(message.value->prefix->nick.empty());

    message = IrcParser::parse(":irc.example.net PRIVMSG #room :hello");
    QVERIFY(message);
    QCOMPARE(text(message.value->prefix->raw), QStringLiteral("irc.example.net"));
    QVERIFY(message.value->prefix->nick.empty());

    message = IrcParser::parse(":n!u@h PRIVMSG #room :hello");
    QVERIFY(message);
    QCOMPARE(text(message.value->prefix->nick), QStringLiteral("n"));
    QCOMPARE(text(message.value->prefix->user), QStringLiteral("u"));
    QCOMPARE(text(message.value->prefix->host), QStringLiteral("h"));
}

void ProtocolTest::parsesTags()
{
    const auto message = IrcParser::parse(
        "@aaa=hello\\sworld;semi=one\\:two;slash=one\\\\two;"
        "lines=one\\rtwo\\nthree;flag :n!u@h PRIVMSG #c :tagged");
    QVERIFY(message);
    QCOMPARE(message.value->tags.size(), std::size_t(5));
    QCOMPARE(text(message.value->tags[0].name), QStringLiteral("aaa"));
    QCOMPARE(text(*message.value->tags[0].value), QStringLiteral("hello world"));
    QCOMPARE(text(*message.value->tags[1].value), QStringLiteral("one;two"));
    QCOMPARE(text(*message.value->tags[2].value), QStringLiteral("one\\two"));
    QCOMPARE(text(*message.value->tags[3].value), QString::fromLatin1("one\rtwo\nthree"));
    QVERIFY(!message.value->tags[4].value.has_value());
    QCOMPARE(text(message.value->command), QStringLiteral("PRIVMSG"));
}

void ProtocolTest::parsesClientOnlyTypingTag()
{
    const auto message = IrcParser::parse(
        "@+typing=active :n!u@h TAGMSG #c");
    QVERIFY(message);
    QCOMPARE(message.value->tags.size(), std::size_t(1));
    QCOMPARE(text(message.value->tags[0].name), QStringLiteral("+typing"));
    QVERIFY(message.value->tags[0].value.has_value());
    QCOMPARE(text(*message.value->tags[0].value), QStringLiteral("active"));
    QCOMPARE(text(message.value->command), QStringLiteral("TAGMSG"));
}

void ProtocolTest::preservesUtf8()
{
    const std::string payload = "PRIVMSG #c :h\xc3\xa9llo \xf0\x9f\xa5\x94";
    const auto message = IrcParser::parse(payload);
    QVERIFY(message);
    QCOMPARE(message.value->parameters[1], payload.substr(payload.find(':') + 1));
}

void ProtocolTest::decodesValidUtf8WireText()
{
    QCOMPARE(ircWireText({}), QString());
    QCOMPARE(ircWireText("hello"), QStringLiteral("hello"));
    QCOMPARE(ircWireText("\xc3\xa9"), QString(QChar(0x00E9)));
    QCOMPARE(ircWireText("h\xc3\xa9llo \xf0\x9f\xa5\x94"),
             QStringLiteral(u"h\u00e9llo \U0001F954"));
}

void ProtocolTest::decodesInvalidUtf8AsLatin1()
{
    const char acute = '\xe9';
    const QString latin1 = ircWireText(std::string_view(&acute, 1));
    QCOMPARE(latin1, QString(QChar(0x00E9)));
    QVERIFY(!latin1.contains(QChar(0xFFFD)));

    const char incomplete = '\xc3';
    const QString loneLead = ircWireText(std::string_view(&incomplete, 1));
    QCOMPARE(loneLead, QString(QChar(0x00C3)));
    QVERIFY(!loneLead.contains(QChar(0xFFFD)));

    const char cafe[] = {'c', 'a', 'f', '\xe9'};
    QCOMPARE(ircWireText(std::string_view(cafe, 4)),
             QStringLiteral("caf") + QChar(0x00E9));
}

void ProtocolTest::framesLatin1ThenUtf8()
{
    IrcFramer framer;
    std::string wire = ":a!u@h PRIVMSG #c :";
    wire.push_back('\xe9');
    wire += "\r\n:b!u@h PRIVMSG #c :ok\r\n";
    const auto result = framer.feed(wire);
    QCOMPARE(result.faults.size(), std::size_t(0));
    QCOMPARE(result.frames.size(), std::size_t(2));

    const auto first = IrcParser::parse(result.frames[0]);
    const auto second = IrcParser::parse(result.frames[1]);
    QVERIFY(first);
    QVERIFY(second);
    QCOMPARE(first.value->parameters[1], std::string(1, '\xe9'));
    QCOMPARE(second.value->parameters[1], std::string("ok"));
}

void ProtocolTest::framesFragmentedAndCoalescedInput()
{
    IrcFramer framer;
    QVERIFY(framer.feed("PIN").frames.empty());
    QVERIFY(framer.feed("G :token").frames.empty());

    auto result = framer.feed("\r\n");
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :token"));

    result = framer.feed("PING :one\r\nPING :two\r\n");
    QCOMPARE(result.frames.size(), std::size_t(2));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :one"));
    QCOMPARE(text(result.frames[1]), QStringLiteral("PING :two"));
}

void ProtocolTest::rejectsNulAndRecovers()
{
    IrcFramer framer;
    std::string wire = "PRIVMSG #c :hel";
    wire.push_back('\0');
    wire += "lo\r\nPING :ok\r\n";
    const auto result = framer.feed(wire);
    QCOMPARE(result.faults.size(), std::size_t(1));
    QCOMPARE(result.faults[0].error, IrcError::InvalidCharacter);
    QCOMPARE(result.faults[0].byteCount, std::size_t(18));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :ok"));
}

void ProtocolTest::rejectsOverlongAndRecovers()
{
    IrcFramer framer;
    const std::size_t overlongBytes = IrcFramer::kMaxInboundClassicFrameBytes - 1;
    std::string wire(overlongBytes, 'X');
    wire += "\r\nPING :complete\r\n";
    auto result = framer.feed(wire);
    QCOMPARE(result.faults.size(), std::size_t(1));
    QCOMPARE(result.faults[0].error, IrcError::TooManyBytes);
    QCOMPARE(result.faults[0].byteCount, overlongBytes);
    QCOMPARE(text(result.faults[0].preview), QString(160, QLatin1Char('X')));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :complete"));

    result = framer.feed(std::string(IrcFramer::kMaxInboundClassicFrameBytes, 'Y'));
    QCOMPARE(result.faults.size(), std::size_t(1));
    QVERIFY(result.frames.empty());
    result = framer.feed("tail\r\nPING :after\r\n");
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :after"));
}

void ProtocolTest::enforcesClassicFrameBoundary()
{
    IrcFramer framer;
    std::string spec = "PING :";
    spec.append(504, 'z');
    spec += "\r\n";
    QCOMPARE(spec.size(), IrcProtocol::maxClassicFrameBytes);
    auto result = framer.feed(spec);
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(result.faults.empty());

    std::string overSpec = "PING :";
    overSpec.append(505, 'z');
    overSpec += "\r\n";
    result = framer.feed(overSpec);
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(result.faults.empty());

    std::string exact = "PING :";
    exact.append(IrcFramer::kMaxInboundClassicFrameBytes - exact.size() - 2, 'z');
    exact += "\r\n";
    QCOMPARE(exact.size(), IrcFramer::kMaxInboundClassicFrameBytes);
    result = framer.feed(exact);
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(result.faults.empty());

    std::string tooLong = "PING :";
    tooLong.append(IrcFramer::kMaxInboundClassicFrameBytes - tooLong.size() - 1, 'z');
    tooLong += "\r\nPING :short\r\n";
    result = framer.feed(tooLong);
    QCOMPARE(result.faults.size(), std::size_t(1));
    QCOMPARE(result.faults[0].error, IrcError::TooManyBytes);
    QCOMPARE(result.faults[0].byteCount,
             IrcFramer::kMaxInboundClassicFrameBytes - 1);
    QVERIFY(text(result.faults[0].preview).startsWith(QStringLiteral("PING :")));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :short"));
}

void ProtocolTest::acceptsTaggedClassicFrame()
{
    IrcFramer framer;
    std::string tagged = "@label=123 ";
    tagged += "PING :";
    tagged.append(504, 'z');
    tagged += "\r\n";
    const auto result = framer.feed(tagged);
    QVERIFY(result.faults.empty());
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(IrcParser::parse(result.frames[0]));
}

void ProtocolTest::enforcesTagSectionBoundary()
{
    IrcFramer framer;
    std::string exact = "@";
    exact.append(IrcFramer::kMaxTagSectionBytes, 'a');
    exact += " PING :ok\r\n";
    auto result = framer.feed(exact);
    QVERIFY(result.faults.empty());
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(IrcParser::parse(result.frames[0]));

    std::string tooLong = "@";
    tooLong.append(IrcFramer::kMaxTagSectionBytes + 1, 'a');
    tooLong += " PING :no\r\nPING :recovered\r\n";
    result = framer.feed(tooLong);
    QCOMPARE(result.faults.size(), std::size_t(1));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :recovered"));
}

void ProtocolTest::buildsRegistration()
{
    auto result = IrcCommandBuilder::registration("alice", "aliceuser", "Omairc User");
    QVERIFY(result);
    QCOMPARE(text(*result.value),
             QStringLiteral("NICK alice\r\nUSER aliceuser 8 * :Omairc User\r\n"));
    QVERIFY(result.value->find("HELLO") == std::string::npos);

    result = IrcCommandBuilder::registration("alice", "aliceuser", "Omairc User", "s3cret");
    QVERIFY(result);
    QCOMPARE(text(*result.value),
             QStringLiteral("PASS s3cret\r\nNICK alice\r\n"
                            "USER aliceuser 8 * :Omairc User\r\n"));
}

void ProtocolTest::rejectsInvalidRegistration()
{
    QVERIFY(!IrcCommandBuilder::registration("", "user", "real"));
    QVERIFY(!IrcCommandBuilder::registration("nick", "", "real"));
    QVERIFY(!IrcCommandBuilder::registration("ni ck", "user", "real"));
    QVERIFY(!IrcCommandBuilder::registration("nick", "us er", "real"));
    QVERIFY(!IrcCommandBuilder::registration("nick", "user", "real", "pass word"));
    QVERIFY(!IrcCommandBuilder::registration("nick\nnick", "user", "real"));

    std::string password = "p";
    password.push_back('\0');
    password += "ass";
    QVERIFY(!IrcCommandBuilder::registration("nick", "user", "real", password));
}

void ProtocolTest::rejectsOutboundInjection()
{
    QVERIFY(!IrcCommandBuilder::line("PRIVMSG #c :hi\r\nQUIT"));
    QVERIFY(!IrcCommandBuilder::line("PRIVMSG #c :hi\nQUIT"));
    QVERIFY(!IrcCommandBuilder::line("PRIVMSG #c :hi\rQUIT"));
    std::string withNul = "PRIVMSG #c :hi";
    withNul.push_back('\0');
    withNul += "there";
    QVERIFY(!IrcCommandBuilder::line(withNul));

    const auto result = IrcCommandBuilder::line("JOIN #chan");
    QVERIFY(result);
    QCOMPARE(text(*result.value), QStringLiteral("JOIN #chan\r\n"));
}

void ProtocolTest::enforcesOutboundBoundary()
{
    auto result = IrcCommandBuilder::line(std::string(510, 'A'));
    QVERIFY(result);
    QCOMPARE(result.value->size(), std::size_t(512));

    result = IrcCommandBuilder::line(std::string(511, 'A'));
    QVERIFY(!result);
}

void ProtocolTest::privmsgUsesIrcv3TimeTag()
{
    const auto parsed = IrcParser::parse(
        "@time=2011-10-19T16:40:51.620Z :n!u@h PRIVMSG #c :hello");
    QVERIFY(parsed);
    const std::vector<IrcEvent> events = translate(*parsed.value);
    QCOMPARE(events.size(), std::size_t(1));
    const auto *message = std::get_if<IrcMessageEvent>(&events.front());
    QVERIFY(message);
    QCOMPARE(message->body, QStringLiteral("hello"));
    QCOMPARE(message->timestamp.toUTC().toMSecsSinceEpoch(),
             exampleServerTime().toMSecsSinceEpoch());
}

void ProtocolTest::privmsgWithoutTimeUsesCurrentUtc()
{
    const auto parsed = IrcParser::parse(":n!u@h PRIVMSG #c :hello");
    QVERIFY(parsed);
    const std::vector<IrcEvent> events = translate(*parsed.value);
    QCOMPARE(events.size(), std::size_t(1));
    const auto *message = std::get_if<IrcMessageEvent>(&events.front());
    QVERIFY(message);
    QVERIFY(isNearCurrentUtc(message->timestamp));
    QVERIFY(message->timestamp.toUTC().toMSecsSinceEpoch()
            != exampleServerTime().toMSecsSinceEpoch());
}

void ProtocolTest::privmsgInvalidTimeUsesCurrentUtc()
{
    const auto parsed = IrcParser::parse(
        "@time=not-a-timestamp :n!u@h PRIVMSG #c :hello");
    QVERIFY(parsed);
    const std::vector<IrcEvent> events = translate(*parsed.value);
    QCOMPARE(events.size(), std::size_t(1));
    const auto *message = std::get_if<IrcMessageEvent>(&events.front());
    QVERIFY(message);
    QVERIFY(isNearCurrentUtc(message->timestamp));
    QVERIFY(message->timestamp.toUTC().toMSecsSinceEpoch()
            != exampleServerTime().toMSecsSinceEpoch());
}

void ProtocolTest::actionAndTypingUseIrcv3TimeTag()
{
    const auto action = IrcParser::parse(
        "@time=2011-10-19T16:40:51.620Z :n!u@h PRIVMSG #c :\x01"
        "ACTION waves\x01");
    QVERIFY(action);
    const std::vector<IrcEvent> actionEvents = translate(*action.value);
    QCOMPARE(actionEvents.size(), std::size_t(1));
    const auto *actionEvent = std::get_if<IrcActionEvent>(&actionEvents.front());
    QVERIFY(actionEvent);
    QCOMPARE(actionEvent->body, QStringLiteral("waves"));
    QCOMPARE(actionEvent->timestamp.toUTC().toMSecsSinceEpoch(),
             exampleServerTime().toMSecsSinceEpoch());

    const auto typing = IrcParser::parse(
        "@time=2011-10-19T16:40:51.620Z;+typing=active :n!u@h TAGMSG #c");
    QVERIFY(typing);
    const std::vector<IrcEvent> typingEvents = translate(*typing.value);
    QCOMPARE(typingEvents.size(), std::size_t(1));
    const auto *typingEvent = std::get_if<IrcTypingEvent>(&typingEvents.front());
    QVERIFY(typingEvent);
    QCOMPARE(typingEvent->phase, IrcTypingPhase::Active);
    QCOMPARE(typingEvent->receivedAt.toUTC().toMSecsSinceEpoch(),
             exampleServerTime().toMSecsSinceEpoch());
}

void ProtocolTest::buildsJoin()
{
    auto result = IrcCommandBuilder::join("#a");
    QVERIFY(result);
    QCOMPARE(text(*result.value), QStringLiteral("JOIN #a\r\n"));

    result = IrcCommandBuilder::join("#a", "pword");
    QVERIFY(result);
    QCOMPARE(text(*result.value), QStringLiteral("JOIN #a pword\r\n"));

    result = IrcCommandBuilder::join("&local", "secret");
    QVERIFY(result);
    QCOMPARE(text(*result.value), QStringLiteral("JOIN &local secret\r\n"));
}

void ProtocolTest::rejectsInvalidJoin()
{
    QVERIFY(!IrcCommandBuilder::join(""));
    QVERIFY(!IrcCommandBuilder::join("#a b"));
    QVERIFY(!IrcCommandBuilder::join("#a,b"));
    QVERIFY(!IrcCommandBuilder::join("#a", "p word"));
    QVERIFY(!IrcCommandBuilder::join("#a", "x,y"));

    auto unkeyedEmpty = IrcCommandBuilder::join("#a", {});
    QVERIFY(unkeyedEmpty);
    QCOMPARE(text(*unkeyedEmpty.value), QStringLiteral("JOIN #a\r\n"));

    std::string withCr = "#a";
    withCr.push_back('\r');
    QVERIFY(!IrcCommandBuilder::join(withCr));

    std::string withLf = "#a";
    withLf.push_back('\n');
    QVERIFY(!IrcCommandBuilder::join(withLf));

    std::string withNul = "#a";
    withNul.push_back('\0');
    QVERIFY(!IrcCommandBuilder::join(withNul));

    std::string keyWithNul = "k";
    keyWithNul.push_back('\0');
    keyWithNul += "ey";
    QVERIFY(!IrcCommandBuilder::join("#a", keyWithNul));

    QVERIFY(!IrcCommandBuilder::join(":chan"));
    QVERIFY(!IrcCommandBuilder::join("#ok", ":key"));

    const auto tooLong = IrcCommandBuilder::join(std::string(508, 'A'));
    QVERIFY(!tooLong);
}

int runProtocolTests(int argc, char **argv)
{
    ProtocolTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_protocol.moc"
