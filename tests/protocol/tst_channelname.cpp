#include <QTest>

#include "ircchannelname.h"
#include "ircserverfeatures.h"

class ChannelNameTest : public QObject
{
    Q_OBJECT

private slots:
    void advertisedTypesDecideThePrefix();
    void boundariesAndTrim();
};

void ChannelNameTest::advertisedTypesDecideThePrefix()
{
    IrcServerFeatures hash;
    hash.applyToken("CHANTYPES=#");
    QCOMPARE(ircChannelNameAt(QStringLiteral("see #desktop now"), 4, hash),
             QStringLiteral("#desktop"));
    QCOMPARE(ircChannelNameAt(QStringLiteral("see &local now"), 4, hash),
             QString());

    IrcServerFeatures amp;
    amp.applyToken("CHANTYPES=&");
    QCOMPARE(ircChannelNameAt(QStringLiteral("see #desktop now"), 4, amp),
             QString());
    QCOMPARE(ircChannelNameAt(QStringLiteral("see &local now"), 4, amp),
             QStringLiteral("&local"));

    IrcServerFeatures dollar;
    dollar.applyToken("CHANTYPES=$");
    const QString dollarText = QStringLiteral("try $secret and #nope");
    QCOMPARE(ircChannelNameAt(dollarText, dollarText.indexOf(QLatin1Char('$')), dollar),
             QStringLiteral("$secret"));
    QCOMPARE(ircChannelNameAt(dollarText, dollarText.indexOf(QLatin1Char('#')), dollar),
             QString());

    IrcServerFeatures empty;
    empty.applyToken("CHANTYPES=");
    QCOMPARE(ircChannelNameAt(QStringLiteral("see #desktop"), 4, empty), QString());
}

void ChannelNameTest::boundariesAndTrim()
{
    IrcServerFeatures features;
    features.applyToken("CHANTYPES=#");

    const QString glued = QStringLiteral("foo#desktop");
    QCOMPARE(ircChannelNameAt(glued, glued.indexOf(QLatin1Char('#')), features),
             QString());

    const QString url = QStringLiteral("https://example.com/#desktop");
    QCOMPARE(ircChannelNameAt(url, url.indexOf(QLatin1Char('#')), features),
             QString());

    const QString lone = QStringLiteral("just # here");
    QCOMPARE(ircChannelNameAt(lone, lone.indexOf(QLatin1Char('#')), features),
             QString());

    const QString paren = QStringLiteral("(#desktop).");
    QCOMPARE(ircChannelNameAt(paren, paren.indexOf(QLatin1Char('#')), features),
             QStringLiteral("#desktop"));
    QCOMPARE(ircChannelNameAt(paren, paren.size() - 1, features), QString());

    const QString bang = QStringLiteral("join #foo!");
    QCOMPARE(ircChannelNameAt(bang, bang.indexOf(QLatin1Char('#')), features),
             QStringLiteral("#foo"));
    QCOMPARE(ircChannelNameAt(bang, bang.size() - 1, features), QString());

    const QString plus = QStringLiteral("see #c++ now");
    QCOMPARE(ircChannelNameAt(plus, plus.indexOf(QLatin1String("#c++")), features),
             QStringLiteral("#c++"));

    const QVector<IrcChannelNameSpan> pair =
        ircChannelNameSpans(QStringLiteral("try #foo,#bar please"), features);
    QCOMPARE(pair.size(), 2);
    QCOMPARE(pair.at(0).name, QStringLiteral("#foo"));
    QCOMPARE(pair.at(1).name, QStringLiteral("#bar"));

    QCOMPARE(ircChannelNameAt(QStringLiteral("#foo_(bar)"), 0, features),
             QStringLiteral("#foo_(bar)"));
}

int runChannelNameTests(int argc, char **argv)
{
    ChannelNameTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_channelname.moc"
