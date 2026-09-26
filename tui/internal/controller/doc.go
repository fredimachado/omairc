// Package controller is the Go port of IrcController from
// src/irc/irccontroller.h and src/irc/irccontroller.cpp.
//
// It is the single API the Bubble Tea shell calls: it owns the live IRC
// sessions, folds translated irc.Events into the irc.EventReducer, decides the
// sidebar order and the selection, and exposes immutable snapshots of the
// conversation list, the selected transcript, and the selected channel's
// members. It also owns a minimal per-network Status ring buffer.
//
// The package is Phase 2 only. It ports the state-model subset of the Qt
// controller and deliberately leaves the command layer, persistence, playback,
// channel list, monitor, ignore/highlight, autoaway, and reply routing to the
// later phases named in seams.go. Nothing here reads the wall clock: every
// timestamp comes from the injected session.Clock, and the package imports
// only the standard library plus the module's internal/irc and internal/session
// packages.
//
// A Controller is not safe for concurrent use. The Qt original lives on one
// thread; drive this from one goroutine (the shell's update loop) the same way.
package controller
