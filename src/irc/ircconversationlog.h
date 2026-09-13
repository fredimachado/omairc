#pragma once

#include <QDateTime>
#include <QString>

#include <memory>
#include <vector>

class QTemporaryDir;

struct IrcTranscriptLine
{
    QDateTime timestamp;
    QString author;
    QString kind;
    QString body;
    QString msgid;
};

class IrcConversationLog
{
public:
    IrcConversationLog();
    explicit IrcConversationLog(QString root);
    ~IrcConversationLog();

    static QString defaultRoot();
    void setRoot(QString root);
    const QString &root() const;
    QString pathFor(const QString &networkId, const QString &target) const;

    bool append(const QString &networkId,
                const QString &target,
                const IrcTranscriptLine &line);
    std::vector<IrcTranscriptLine> readTail(const QString &networkId,
                                            const QString &target,
                                            int maxLines) const;

private:
    QString m_root;
    std::unique_ptr<QTemporaryDir> m_scratch;
};
