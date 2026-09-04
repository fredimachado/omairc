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

#include <QTest>
#include <QString>

#include "irccommandbuilder.h"
#include "ircframer.h"
#include "ircparser.h"

#include <string>
#include <vector>

namespace
{
QString text(const std::string& value)
{
    return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
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
    QCOMPARE(result.errors.size(), std::size_t(1));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :ok"));
}

void ProtocolTest::rejectsOverlongAndRecovers()
{
    IrcFramer framer;
    std::string wire(513, 'X');
    wire += "\r\nPING :complete\r\n";
    auto result = framer.feed(wire);
    QCOMPARE(result.errors.size(), std::size_t(1));
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :complete"));

    result = framer.feed(std::string(512, 'Y'));
    QCOMPARE(result.errors.size(), std::size_t(1));
    QVERIFY(result.frames.empty());
    result = framer.feed("tail\r\nPING :after\r\n");
    QCOMPARE(result.frames.size(), std::size_t(1));
    QCOMPARE(text(result.frames[0]), QStringLiteral("PING :after"));
}

void ProtocolTest::enforcesClassicFrameBoundary()
{
    IrcFramer framer;
    std::string exact = "PING :";
    exact.append(504, 'z');
    exact += "\r\n";
    QCOMPARE(exact.size(), IrcFramer::kMaxClassicFrameBytes);
    auto result = framer.feed(exact);
    QCOMPARE(result.frames.size(), std::size_t(1));

    std::string tooLong = "PING :";
    tooLong.append(505, 'z');
    tooLong += "\r\nPING :short\r\n";
    result = framer.feed(tooLong);
    QCOMPARE(result.errors.size(), std::size_t(1));
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
    QVERIFY(result.errors.empty());
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
    QVERIFY(result.errors.empty());
    QCOMPARE(result.frames.size(), std::size_t(1));
    QVERIFY(IrcParser::parse(result.frames[0]));

    std::string tooLong = "@";
    tooLong.append(IrcFramer::kMaxTagSectionBytes + 1, 'a');
    tooLong += " PING :no\r\nPING :recovered\r\n";
    result = framer.feed(tooLong);
    QCOMPARE(result.errors.size(), std::size_t(1));
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

int runProtocolTests(int argc, char **argv)
{
    ProtocolTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_protocol.moc"
