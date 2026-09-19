#include <QRegularExpression>
#include <QStringList>
#include <QTest>

#include "irctextformatter.h"

namespace {

const QChar kBold(0x02);
const QChar kColor(0x03);
const QChar kHexColor(0x04);
const QChar kReset(0x0f);
const QChar kMonospace(0x11);
const QChar kReverse(0x16);
const QChar kItalic(0x1d);
const QChar kStrikethrough(0x1e);
const QChar kUnderline(0x1f);

const auto kWrapper = QStringLiteral("<span style=\"white-space: pre-wrap;\">");

QString wrap(const QString &inner)
{
    return kWrapper + inner + QStringLiteral("</span>");
}

QString formattedIrcBody()
{
    return QChar(0x02) + QStringLiteral("bold") + QChar(0x0f)
        + QStringLiteral(" / ") + QChar(0x03) + QStringLiteral("04red");
}

bool htmlTagsAreAllowed(const QString &html)
{
    if (html.indexOf(QLatin1String("<a")) >= 0)
        return false;
    if (!html.startsWith(kWrapper))
        return false;
    if (html.lastIndexOf(QLatin1String("</span>")) != html.size() - 7)
        return false;
    if (html.indexOf(QLatin1String("<span")) != 0)
        return false;
    if (html.indexOf(QLatin1String("<span"), 1) >= 0)
        return false;

    static const QRegularExpression re(
        QStringLiteral("</?([A-Za-z][A-Za-z0-9]*)\\b([^>]*)>"));
    QRegularExpressionMatchIterator it = re.globalMatch(html);
    while (it.hasNext()) {
        const QRegularExpressionMatch match = it.next();
        const QString tag = match.captured(1).toLower();
        if (tag != QLatin1String("b") && tag != QLatin1String("i")
            && tag != QLatin1String("u") && tag != QLatin1String("span")) {
            return false;
        }
        const QString full = match.captured(0);
        if (tag == QLatin1String("span") && full.at(1) != QLatin1Char('/')) {
            if (full != kWrapper)
                return false;
        } else if (!match.captured(2).isEmpty()) {
            return false;
        }
    }
    return true;
}

}

class IrcTextFormatterTest : public QObject
{
    Q_OBJECT

private slots:
    void plainStripsColorsAndCodes();
    void emphasizedBoldItalicUnderline();
    void emphasizedNestedClosesInnerFirst();
    void emphasizedColorOnly();
    void emphasizedLiteralHtmlIsEscaped();
    void emphasizedReverseDoesNotSplitBold();
    void emphasizedHexColor();
    void emphasizedExtraCodesStayInsideBold();
    void emphasizedPreservesDoubleSpaces();
    void emphasizedFailClosedAllowlist();
    void hasIrcEmphasisIgnoresStrippedCodes();
    void stripIrcColorsMatchesOptionalDigitsAndHex();
};

void IrcTextFormatterTest::plainStripsColorsAndCodes()
{
    IrcTextFormatter fmt;
    const QString bold = QStringLiteral("hello ") + kBold + QStringLiteral("world")
        + kBold;
    QCOMPARE(fmt.plainIrcText(bold), QStringLiteral("hello world"));
    QCOMPARE(fmt.plainIrcText(kColor + QStringLiteral("04red") + kColor),
             QStringLiteral("red"));
    QCOMPARE(fmt.plainIrcText(kHexColor + QStringLiteral("FF0000red")),
             QStringLiteral("red"));
}

void IrcTextFormatterTest::emphasizedBoldItalicUnderline()
{
    IrcTextFormatter fmt;
    const QString bold = QStringLiteral("hello ") + kBold + QStringLiteral("world")
        + kBold;
    QCOMPARE(fmt.emphasizedIrcText(bold), wrap(QStringLiteral("hello <b>world</b>")));
    QVERIFY(fmt.hasIrcEmphasis(bold));

    QCOMPARE(fmt.emphasizedIrcText(kItalic + QStringLiteral("italic") + kItalic),
             wrap(QStringLiteral("<i>italic</i>")));
    QCOMPARE(fmt.emphasizedIrcText(kUnderline + QStringLiteral("under") + kUnderline),
             wrap(QStringLiteral("<u>under</u>")));
}

void IrcTextFormatterTest::emphasizedNestedClosesInnerFirst()
{
    IrcTextFormatter fmt;
    const QString nested = kBold + QStringLiteral("bold") + kItalic
        + QStringLiteral("italic") + kItalic + kBold;
    const QString html = fmt.emphasizedIrcText(nested);
    QVERIFY(html.contains(QLatin1String("<b>")));
    QVERIFY(html.contains(QLatin1String("<i>")));
    QVERIFY(!html.contains(QLatin1String("</b></i>")));
    QVERIFY(!html.contains(QRegularExpression(
        QStringLiteral("<b>[^<]*<i>[\\s\\S]*</b>\\s*</i>"))));
    QCOMPARE(html, wrap(QStringLiteral("<b>bold</b><b><i>italic</i></b><b></b>")));
}

void IrcTextFormatterTest::emphasizedColorOnly()
{
    IrcTextFormatter fmt;
    const QString colorOnly = kColor + QStringLiteral("04red") + kColor;
    QCOMPARE(fmt.plainIrcText(colorOnly), QStringLiteral("red"));
    QCOMPARE(fmt.emphasizedIrcText(colorOnly), wrap(QStringLiteral("red")));
    QVERIFY(!fmt.hasIrcEmphasis(colorOnly));
}

void IrcTextFormatterTest::emphasizedLiteralHtmlIsEscaped()
{
    IrcTextFormatter fmt;
    const QString html = fmt.emphasizedIrcText(QStringLiteral("<b>not html</b>"));
    QVERIFY(html.contains(QLatin1String("&lt;b&gt;")));
    QVERIFY(html.contains(QLatin1String("&lt;/b&gt;")));
    QVERIFY(!html.contains(QLatin1String("<b>")));
    QVERIFY(!html.contains(QLatin1String("</b>")));
    QCOMPARE(html, wrap(QStringLiteral("&lt;b&gt;not html&lt;/b&gt;")));
}

void IrcTextFormatterTest::emphasizedReverseDoesNotSplitBold()
{
    IrcTextFormatter fmt;
    const QString reverse = kBold + QStringLiteral("a") + kReverse
        + QStringLiteral("b") + kBold;
    const QString html = fmt.emphasizedIrcText(reverse);
    QVERIFY(html.contains(QLatin1String("<b>ab</b>")));
    QVERIFY(!html.contains(kReverse));
    QVERIFY(!html.contains(QLatin1String("</b><b>")));
    QCOMPARE(html, wrap(QStringLiteral("<b>ab</b>")));
}

void IrcTextFormatterTest::emphasizedHexColor()
{
    IrcTextFormatter fmt;
    const QString hex = kHexColor + QStringLiteral("FF0000red");
    QCOMPARE(fmt.plainIrcText(hex), QStringLiteral("red"));
    const QString html = fmt.emphasizedIrcText(hex);
    QVERIFY(html.contains(QLatin1String("red")));
    QVERIFY(!html.contains(QLatin1String("FF0000")));
    QCOMPARE(html, wrap(QStringLiteral("red")));
}

void IrcTextFormatterTest::emphasizedExtraCodesStayInsideBold()
{
    IrcTextFormatter fmt;
    const QString extra = kBold + QStringLiteral("a") + kMonospace
        + QStringLiteral("b") + kStrikethrough + QStringLiteral("c") + kBold;
    QCOMPARE(fmt.emphasizedIrcText(extra), wrap(QStringLiteral("<b>abc</b>")));
}

void IrcTextFormatterTest::emphasizedPreservesDoubleSpaces()
{
    IrcTextFormatter fmt;
    const QString spaced = QStringLiteral("a  ") + kBold + QStringLiteral("b") + kBold;
    QVERIFY(fmt.emphasizedIrcText(spaced).contains(QLatin1String("a  <b>b</b>")));
    QCOMPARE(fmt.emphasizedIrcText(spaced), wrap(QStringLiteral("a  <b>b</b>")));
}

void IrcTextFormatterTest::emphasizedFailClosedAllowlist()
{
    IrcTextFormatter fmt;
    const QStringList samples = {
        QStringLiteral("hello ") + kBold + QStringLiteral("world") + kBold,
        kItalic + QStringLiteral("italic") + kItalic,
        kUnderline + QStringLiteral("under") + kUnderline,
        kBold + QStringLiteral("bold") + kItalic + QStringLiteral("italic")
            + kItalic + kBold,
        kColor + QStringLiteral("04red") + kColor,
        QStringLiteral("<b>not html</b>"),
        QStringLiteral("<a href=\"https://evil.example\">x</a>") + kBold
            + QStringLiteral("y") + kBold,
        kReverse + QStringLiteral("flip") + kReverse,
        QString() + kBold + kItalic + kUnderline + kReset,
        kBold + QStringLiteral("a") + kReverse + QStringLiteral("b") + kBold,
        kHexColor + QStringLiteral("FF0000red"),
        kBold + QStringLiteral("a") + kMonospace + QStringLiteral("b")
            + kStrikethrough + QStringLiteral("c") + kBold,
        QStringLiteral("a  ") + kBold + QStringLiteral("b") + kBold,
        formattedIrcBody(),
    };
    for (const QString &sample : samples) {
        const QString html = fmt.emphasizedIrcText(sample);
        QVERIFY2(htmlTagsAreAllowed(html), qPrintable(html));
    }

    const QString evil = QStringLiteral("<a href=\"https://evil.example\">x</a>")
        + kBold + QStringLiteral("y") + kBold;
    QCOMPARE(fmt.emphasizedIrcText(evil),
             wrap(QStringLiteral("&lt;a href=\"https://evil.example\"&gt;x&lt;/a&gt;<b>y</b>")));
}

void IrcTextFormatterTest::hasIrcEmphasisIgnoresStrippedCodes()
{
    IrcTextFormatter fmt;
    QVERIFY(!fmt.hasIrcEmphasis(kReverse + QStringLiteral("flip") + kReverse));
    QVERIFY(!fmt.hasIrcEmphasis(kMonospace + QStringLiteral("mono") + kMonospace));
    QVERIFY(!fmt.hasIrcEmphasis(kColor + QStringLiteral("04red") + kColor));
    QVERIFY(fmt.hasIrcEmphasis(QStringLiteral("hello ") + kBold
                               + QStringLiteral("world") + kBold));
}

void IrcTextFormatterTest::stripIrcColorsMatchesOptionalDigitsAndHex()
{
    IrcTextFormatter fmt;
    QCOMPARE(fmt.stripIrcColors(kColor + QStringLiteral("4red")), QStringLiteral("red"));
    QCOMPARE(fmt.stripIrcColors(kColor + QStringLiteral("04,02red") + kColor),
             QStringLiteral("red"));
    QCOMPARE(fmt.stripIrcColors(kColor + QStringLiteral(",04red")),
             QStringLiteral(",04red"));
    QCOMPARE(fmt.stripIrcColors(kHexColor + QStringLiteral("aabbcc,ddeeffx")),
             QStringLiteral("x"));
    QCOMPARE(fmt.stripIrcColors(kHexColor + QStringLiteral("FF00red")),
             QStringLiteral("FF00red"));
    QCOMPARE(fmt.plainIrcText(kBold + QStringLiteral("a") + kReset
                              + QStringLiteral("b")),
             QStringLiteral("ab"));
}

int runIrcTextFormatterTests(int argc, char **argv)
{
    IrcTextFormatterTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_irctextformatter.moc"
