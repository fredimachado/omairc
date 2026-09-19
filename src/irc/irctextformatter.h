#pragma once

#include <QObject>
#include <QString>

void omaircRegisterIrcTextFormatter();

class IrcTextFormatter : public QObject
{
    Q_OBJECT

public:
    explicit IrcTextFormatter(QObject *parent = nullptr);

    Q_INVOKABLE QString stripIrcColors(const QString &text) const;
    Q_INVOKABLE QString plainIrcText(const QString &text) const;
    Q_INVOKABLE QString escapeHtml(const QString &text) const;
    Q_INVOKABLE bool hasIrcEmphasis(const QString &text) const;
    Q_INVOKABLE QString emphasizedIrcText(const QString &text) const;
};
