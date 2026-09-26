// Package demo is the Go port of IrcDemoServer from
// src/irc/ircdemoserver.h and src/irc/ircdemoserver.cpp.
//
// It is pure in-memory test scaffolding: it seeds the identical two-network
// demo world (omarchy and oftc) over session.LoopbackTransport, drives a
// controller through registration, channel state, presence, and transcript
// replay, and optionally auto-answers the client's wire traffic the way the
// Qt demo server connects IrcLoopbackTransport::frameWritten. Nothing here
// touches the network or the filesystem; it imports only the standard library
// plus this module's internal controller, irc, and session packages.
//
// The seed data, the wire bytes, and the reply order are a byte-for-byte port
// of the C++ demo server. Where the C++ iterates a QSet (the away and status
// unions), the Go port emits the same set of lines in a deterministic sorted
// order.
package demo
