# IRC Connectivity Implementation Plan

## Objective

Add clean, testable IRC connectivity to Omairc while preserving its small Qt
Quick interface and Omarchy-native behavior. The first release will activate
one network at a time, but its state and APIs must be multi-network-ready.

Reuse the strongest ideas and selected implementation details from IRCClient:

- IRC frame validation and 512-byte limits
- Incremental receive buffering and CRLF frame extraction
- IRC message and prefix parsing
- Registration command construction
- Automatic `PING`/`PONG`
- Defensive socket lifecycle and partial-write handling
- Existing protocol and lifecycle test cases where they remain applicable

Do not embed IRCClient as-is. Omairc should use Qt-native asynchronous networking,
typed events, injectable boundaries, and models designed for QML.

## Initial scope

The first complete release will support:

- One configured IRC network at a time
- Plain TCP and TLS, with TLS enabled by default
- Nick, username, real name, and optional server password
- IRCv3 capability negotiation
- Optional SASL PLAIN authentication
- Joining and leaving channels
- Channel and direct messages
- `/me` actions
- Topics and channel member lists
- Nick, join, part, quit, kick, and basic mode events
- Connection status and actionable errors
- Controlled reconnection after an unexpected disconnect

The initial release will not include:

- Multiple simultaneous networks
- DCC, file transfer, or voice/video
- Bouncer-specific history synchronization
- Plugin or scripting systems
- A comprehensive raw-command UI
- Persistent local message history

These boundaries keep the first implementation aligned with Omairc's
dead-simple product direction without preventing later extension.

## Multi-network readiness

Supporting one active network in the initial UI must not create global
single-network assumptions. From the first implementation:

- Every configured network has a stable, opaque `networkId` that does not
  change when its display name or server address changes.
- Every session, domain event, conversation, message, member collection, and
  persisted network setting is associated with a `networkId`.
- A conversation is identified by the composite key `(networkId,
  normalizedTarget)`, never by a channel or nick alone.
- Target normalization uses the owning session's advertised `CASEMAPPING`;
  server features and identity comparisons are never shared across networks.
- Connection state, current nick, capabilities, reconnect policy, credentials,
  and channel membership belong to an individual `IrcSession`.
- Outbound actions are routed through the session that owns the selected
  conversation.
- Models may aggregate entries from sessions, but model data must retain its
  network identity even when the initial UI does not display it.

Introduce an `IrcSessionManager` boundary from the start. Initially it will own
at most one active session; later it can own several without changing the
protocol, transport, reducer, session, or model contracts. Enabling simultaneous
networks should then primarily require profile management and UI grouping,
rather than a state-model rewrite.

## Architecture

Keep the current `Backend` focused on desktop integration. Introduce a separate
IRC subsystem with four layers.

### 1. Protocol core

A Qt-independent C++17 library for deterministic protocol work:

- `IrcMessage`: parsed tags, optional prefix, command, and parameters
- `IrcPrefix`: nick, user, host, and raw prefix
- `IrcParser`: parses one complete IRC line into `IrcMessage`
- `IrcFramer`: accepts arbitrary byte chunks and emits complete IRC frames
- `IrcCommandBuilder`: validates and constructs outbound commands
- `IrcCaseMapping`: compares nick and channel identifiers using the server's
  advertised `CASEMAPPING`
- `IrcServerFeatures`: interprets relevant `005` tokens such as `CHANTYPES`,
  `PREFIX`, `CASEMAPPING`, and `NICKLEN`

This layer must not depend on QObject, sockets, QML, global state, callbacks, or
console output. It should accept values and return values or explicit errors.

Adapt the tested parsing, framing, and validation behavior from IRCClient rather
than copying its console handlers or raw hook mechanism. Unlike IRCClient, the
parser must preserve IRCv3 message tags instead of discarding tagged messages.

### 2. Transport

Define an injectable `IrcTransport` QObject interface:

- Connect to host and port in plain or TLS mode
- Emit connected, encrypted, disconnected, bytes-received, and error events
- Queue complete writes without blocking the GUI thread
- Expose explicit shutdown and connection state

Implement production transport with `QSslSocket`. Use the same implementation
for plain TCP by selecting the appropriate connection method. Never bypass
certificate errors silently; surface them as connection failures with useful
details.

Tests will inject `FakeIrcTransport`, allowing session behavior to be exercised
without DNS, external servers, sleeps, or threads.

### 3. Session and state

Each `IrcSession` will have an immutable `networkId` and own one transport, the
framer, registration state machine, and that network's state. It will:

- Perform `CAP LS`, capability selection, optional SASL, `NICK`, and `USER`
- Send `PONG` immediately when receiving `PING`
- Join configured channels after registration
- Convert parsed messages into typed domain events
- Update channel, membership, topic, and conversation state
- Track expected versus unexpected disconnects
- Apply bounded exponential reconnect delays that can always be cancelled
- Surface protocol, authentication, TLS, and network failures explicitly

Keep protocol interpretation separate from model mutation. A small
`IrcEventReducer` should transform typed events into state changes. This makes
JOIN/PART/NICK/QUIT/NAMES and reconnection behavior testable without QML or a
live socket.

Use IRC identifier comparison rather than ordinary case-sensitive string
comparison. Do not assume every channel starts with `#`; use the server's
`CHANTYPES` value.

`IrcSessionManager` will create, own, find, and stop sessions by `networkId`.
It must not merge per-server capabilities or protocol state. The initial
controller will ask it to activate one configured profile, while its API and
ownership model will permit several active sessions later.

### 4. Qt models and QML bridge

Expose the session through a dedicated `IrcController` QObject registered with
QML independently from `Backend`.

Provide model classes derived from `QAbstractListModel`:

- `ConversationListModel`: channels and direct-message conversations, keyed by
  network and normalized target
- `MessageListModel`: messages and events for the selected conversation
- `MemberListModel`: members of the selected channel

Use stable role names that match the existing interface where practical:
`author`, `time`, `body`, and `kind` for messages; `nick`, `status`, and `away`
for members. Every applicable model must also expose `networkId`; conversations
must expose an unambiguous conversation identifier. Add unread metadata where
needed, but keep display formatting in QML.

All model mutations must occur on the Qt GUI thread. `QSslSocket` and
`IrcSession` should normally live on that thread and rely on Qt's asynchronous
event loop, avoiding a dedicated receive thread entirely.

## Proposed source layout

```text
src/
  irc/
    ircmessage.h
    ircparser.h
    ircparser.cpp
    ircframer.h
    ircframer.cpp
    irccommandbuilder.h
    irccommandbuilder.cpp
    irccasemapping.h
    irccasemapping.cpp
    ircserverfeatures.h
    ircserverfeatures.cpp
    irctransport.h
    qtirctransport.h
    qtirctransport.cpp
    ircsession.h
    ircsession.cpp
    ircsessionmanager.h
    ircsessionmanager.cpp
    ircevent.h
    irceventreducer.h
    irceventreducer.cpp
    irccontroller.h
    irccontroller.cpp
    conversationlistmodel.h
    conversationlistmodel.cpp
    messagelistmodel.h
    messagelistmodel.cpp
    memberlistmodel.h
    memberlistmodel.cpp
tests/
  tests.pro
  protocol/
  session/
  models/
  integration/
  support/
    fakeirctransport.h
    fakeirctransport.cpp
```

Keep test fixtures and fake transports outside production sources.

## Delivery phases

### Phase 1: Test foundation and protocol core

1. Add a Qt Test-based test target that builds independently from the GUI.
2. Port applicable IRCClient parser, framing, command-validation, and malformed
   input test cases.
3. Implement `IrcMessage`, `IrcParser`, `IrcFramer`, and
   `IrcCommandBuilder`.
4. Add IRCv3 tag parsing, including escaping rules.
5. Add tests for fragmented frames, multiple frames per read, overlong frames,
   embedded NUL bytes, invalid commands, CR/LF injection, UTF-8 payload bytes,
   and maximum outbound frame size.

**Exit criteria:** the protocol core has no Qt GUI or network dependency and all
behavior is covered by deterministic unit tests.

### Phase 2: Transport and connection state machine

1. Define `IrcTransport` and implement `QtIrcTransport` with `QSslSocket`.
2. Implement `FakeIrcTransport`.
3. Build `IrcSession` connection, shutdown, registration, and error states,
   with an immutable `networkId`.
4. Implement CAP negotiation and automatic `PING`/`PONG`.
5. Add optional SASL PLAIN without logging or persisting credentials.
6. Add explicit TLS certificate and authentication error reporting.
7. Add `IrcSessionManager`, initially constrained to one active session.
8. Test success, refusal, timeout, remote close, malformed input, cancellation,
   reconnect scheduling, and destruction during connection.

**Exit criteria:** a session can register against a fake transport and a local
test server without blocking the event loop or accessing external services.

### Phase 3: IRC event reduction and models

1. Add typed events for welcome, message, notice, action, join, part, quit,
   nick, kick, topic, names, mode, and server error.
2. Interpret relevant `005` server features.
3. Implement channel and direct-message state with IRC-aware identifier
   comparison and network-scoped composite keys.
4. Implement conversation, message, and member models.
5. Track unread messages and mentions without coupling state to visual
   components.
6. Test NAMES synchronization, duplicate events, nick changes across channels,
   quit removal, self join/part/kick, direct-message creation, and reconnect
   state reset.
7. Test that identical channel and nick names on two synthetic networks remain
   isolated, even though simultaneous live connections are not yet exposed.

**Exit criteria:** recorded IRC message sequences produce the expected models
without constructing the application window.

### Phase 4: QML integration

1. Register `IrcController` in `main.cpp` as a separate QML context property.
2. Replace fixed channel and direct-message navigation with
   `ConversationListModel`.
3. Replace per-conversation mock `ListModel`s with the selected
   `MessageListModel`.
4. Bind the topic, people count, and member panel to live state.
5. Route composer input through the controller:
   - normal text becomes `PRIVMSG`
   - `/me` becomes CTCP ACTION
   - a small explicit command set handles `/join`, `/part`, `/nick`, and `/quit`
6. Resolve every outbound action through the selected conversation's
   `networkId`, rather than a global current session.
7. Show connection progress and errors using the existing calm visual language,
   without adding speculative controls.
8. Preserve keyboard behavior, theme-derived colors, text scaling, and
   single-line Enter-to-send behavior.

Keep the mock data available behind a build-time development option until live
models cover every existing visual state. Remove it only after the live path is
complete.

**Exit criteria:** the existing interface works against a local IRC server and
has no direct socket or protocol logic in QML.

### Phase 5: Hardening and release readiness

1. Add loopback integration tests using a local `QTcpServer`.
2. Add a local TLS test server and checked-in test-only certificate material.
3. Test connection teardown and reconnection under sanitizers where available.
4. Verify no credentials, raw PASS/AUTHENTICATE payloads, or private messages
   are written to logs.
5. Test against at least two real IRC server implementations manually.
6. Confirm behavior for IPv4, IPv6, DNS failure, server throttling, nickname
   collision, unavailable capabilities, and invalid certificates.
7. Update the README with configuration, supported features, and known limits.

**Exit criteria:** local automated tests cover the protocol and lifecycle, the
GUI starts warning-free, and manual network testing demonstrates secure
connect, chat, disconnect, and reconnect behavior.

## Testing strategy

Use three distinct test levels:

| Level | Scope | Network dependency |
|---|---|---|
| Unit | Parser, framing, command building, case mapping, feature parsing, reducer | None |
| Component | `IrcSession` with `FakeIrcTransport`; list model roles and mutations | None |
| Integration | `QtIrcTransport` against local plain and TLS test servers | Loopback only |

Avoid timing-sensitive sleeps. Fake transport and timers should be injectable,
and asynchronous tests should wait for specific Qt signals with bounded
timeouts.

Every fixed bug in parsing, lifecycle, or state reduction should receive a
regression test before or with the fix.

## Build integration

Retain qmake and C++17 initially. Add the IRC sources explicitly to
`omairc.pro`, and create a separate `tests/tests.pro` test target. Do not pull
the IRCClient CMake project into Omairc or compile its console-specific files.

If the source list becomes difficult to maintain, consider a later migration of
the whole application to CMake as a separate change. Do not combine that build
system migration with the first IRC implementation.

The expected validation commands will become:

```sh
bin/build
bin/test
QT_QPA_PLATFORM=offscreen timeout 3 ./build/omairc
```

`bin/test` should build and run only local deterministic tests.

## Configuration and credential handling

Start with one network profile containing a generated stable `networkId`, host,
port, TLS choice, nick, username, real name, and autojoin channels. Persist
non-secret settings with `QSettings`, namespaced by `networkId` rather than by
display name or host.

Represent settings as a collection of network profiles even though the first
release activates only one. A future multi-network release can then permit
several enabled profiles without migrating the settings schema.

Do not persist server or SASL passwords in plain-text settings. Keep passwords
in memory for the first implementation. If persistence is later required, add
Secret Service integration as a separate feature with explicit failure
handling.

## Reuse and provenance

When adapting IRCClient code:

- Copy only protocol behavior that fits the new boundaries.
- Preserve meaningful tests before refactoring implementation details.
- Remove console output, raw function hooks, platform socket wrappers, and
  thread coordination from copied code.
- Retain clear source attribution in files substantially derived from
  IRCClient.
- Document intentional behavior changes, especially IRCv3 tags, TLS, command
  handling, and Qt event-loop ownership.

The IRCClient repository should remain unchanged and independently buildable.

## Definition of done

IRC connectivity is complete when:

- Omairc connects securely to a configured IRC server.
- Registration, optional SASL, autojoin, messaging, topics, and membership
  updates work through live models.
- Network-scoped identifiers are retained through settings, sessions, events,
  reducers, models, selection, and outbound command routing.
- Tests prove that identically named conversations on different synthetic
  networks cannot collide or leak state.
- Disconnects and failures are visible and recoverable.
- The GUI thread is never blocked by DNS, connect, receive, or shutdown work.
- Protocol and session behavior can be tested without launching QML or using an
  external network.
- No network code is added to `Backend`, and no IRC parsing is added to QML.
- Existing theme, scaling, keyboard, and conversation UX behavior is retained.
- Mock conversations are no longer part of the normal production path.

## Future simultaneous-network increment

Once the initial release is stable, simultaneous network support should require:

1. Allowing `IrcSessionManager` to activate more than one enabled profile.
2. Grouping or labeling conversations by network in the sidebar.
3. Showing per-network connection and authentication status.
4. Defining connect-all, disconnect-all, and per-network reconnect behavior.
5. Adding integration tests with two concurrent local servers.

No parser, transport, reducer, session, conversation-key, or settings-schema
redesign should be necessary for this increment.
