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
QString decodeOmaircStorageSegment(QString segment);

bool legacyStoragePathExists(const QString &path,
                             const QString &segment,
                             QString extensionForDeviceCheck = {});
bool legacyStorageDirExists(const QDir &dir,
                            const QString &segment,
                            QString extensionForDeviceCheck = {});

QString storageCanonicalPath(const QString &path);
bool storagePathsSameFile(const QString &left, const QString &right);
bool storageDirSegmentsShareLocation(const QDir &root,
                                     const QString &leftSegment,
                                     const QString &rightSegment);

QString omaircWireStorageSegment(QString text,
                                 QString extensionForDeviceCheck = {});
