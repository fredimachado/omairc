#pragma once

#include "irccommand.h"

#include <QObject>
#include <QString>
#include <QVariantList>
#include <QVector>

struct IrcSlashHit
{
    QString label;
    QString usage;
};

inline bool operator==(const IrcSlashHit& left, const IrcSlashHit& right)
{
    return left.label == right.label && left.usage == right.usage;
}

class IrcSlashProbe
{
public:
    static IrcSlashProbe closed();
    static IrcSlashProbe open(QString needle, QVector<IrcSlashHit> hits);

    bool isOpen() const { return m_open; }
    QString needle() const { return m_needle; }
    const QVector<IrcSlashHit>& hits() const { return m_hits; }
    bool containsLabel(const QString& label) const;

private:
    bool m_open = false;
    QString m_needle;
    QVector<IrcSlashHit> m_hits;
};

class IrcSlashComplete
{
public:
    static IrcSlashProbe project(const QString& composerText,
                                 IrcComposerSurface surface);

private:
    static QString openableNeedle(const QString& composerText);
    static QVector<IrcSlashHit> rank(const QString& foldedNeedle,
                                     IrcComposerSurface surface);
    static int scoreSpec(const QString& foldedNeedle, const IrcVerbSpec& spec);
    static int scoreToken(const QString& foldedNeedle, const QString& token);
};

struct IrcSlashKeyResult
{
    Q_GADGET
    Q_PROPERTY(bool accepted MEMBER accepted CONSTANT)
    Q_PROPERTY(QString insertion MEMBER insertion CONSTANT)
public:
    bool accepted = false;
    QString insertion;
};

class IrcSlashSession : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool open READ open NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList matches READ matches NOTIFY snapshotChanged)
    Q_PROPERTY(int selectedIndex READ selectedIndex WRITE setSelectedIndex
               NOTIFY snapshotChanged)

public:
    explicit IrcSlashSession(QObject *parent = nullptr);

    bool open() const;
    QVariantList matches() const;
    int selectedIndex() const;
    void setSelectedIndex(int index);

    Q_INVOKABLE void sync(const QString& composerText, bool statusConsole);
    Q_INVOKABLE IrcSlashKeyResult routeKey(int key, int modifiers);
    Q_INVOKABLE QString activate(int index);
    Q_INVOKABLE void dismiss();

signals:
    void snapshotChanged();

private:
    QString insertionAt(int index) const;
    QString replacedComposer(int index) const;
    bool tokenPassesSelected() const;
    void applyProbe(const IrcSlashProbe& probe);

    IrcSlashProbe m_probe = IrcSlashProbe::closed();
    int m_selectedIndex = -1;
    QString m_dismissedNeedle;
    QString m_lastText;
};

Q_DECLARE_METATYPE(IrcSlashKeyResult)
