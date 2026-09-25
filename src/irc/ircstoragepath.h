#pragma once

#include <QString>

class IrcCaseMapping;

QString omaircStorageSegment(QString text, QString extensionForDeviceCheck = {});
QString omaircTargetSegment(QString target,
                            const IrcCaseMapping &mapping,
                            QString extensionForDeviceCheck = {});

QString legacyStorageSegment(QString text);
QString decodeLegacySegment(QString text);
