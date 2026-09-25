#pragma once

#include <QString>

class IrcCaseMapping;
class QDir;

QString omaircStorageSegment(QString text, QString extensionForDeviceCheck = {});
QString omaircTargetSegment(QString target,
                            const IrcCaseMapping &mapping,
                            QString extensionForDeviceCheck = {});

QString legacyStorageSegment(QString text);
QString decodeLegacySegment(QString text);

bool legacyStoragePathExists(const QString &path,
                             const QString &segment,
                             QString extensionForDeviceCheck = {});
bool legacyStorageDirExists(const QDir &dir,
                            const QString &segment,
                            QString extensionForDeviceCheck = {});
