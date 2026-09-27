// Package ui hosts the Bubble Tea v2 shell for omairc-tui.
//
// It mirrors src/qml/ plus OmaircWindow.qml as a thin shell over the Go
// IrcController: file-based components instead of one god-object, protocol
// policy kept out of the view, and rendering that reads controller snapshots
// only.
//
// Layout is three columns plus a one-line composer: sidebar, transcript, and
// the member panel when a channel is selected. Tiny terminals degrade to the
// composer alone rather than panicking.
package ui
