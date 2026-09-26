// Package ui hosts the Bubble Tea shell for omairc-tui.
//
// It mirrors src/qml/ plus OmaircWindow.qml as a thin shell over the Go
// IrcController: extract file-based components instead of growing one
// god-object, and keep the view free of protocol policy.
//
// Later phases fill this package; Phase 0 ships only this placeholder so the
// module compiles.
package ui
