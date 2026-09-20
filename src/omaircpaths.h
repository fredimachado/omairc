#ifndef OMAIRCPATHS_H
#define OMAIRCPATHS_H

#include <QString>

QString omaircConfigRoot();
QString omaircStateRoot();
QString omaircMacBundleContentsDir(const QString &executablePath);
QString omaircApplyMacBundleQtPaths();

#endif
