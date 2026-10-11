# Proxy Injection — Recorded Session Explorer
first make a copy of bot/rbc_proxy_listener.py and component parts that are essential in captuirng live data from the connected keep its setting. then unpack where critical logic is and make the server start stop/ from the rust app. self contained. 

Extend the existing working proxy injection/ python with a lightweight, standalone recorded-session manager and inspector.

Use only the injection component, with existing endpoints and logging / formatting. detect the type like adi . but ignore about running or sending for now. but allow it to be extended to send commands easily through the development monitor . Python backend, rust front end for performance. 

**First inspect the existing codebase.** Reuse its message interception and decoding functionality. Do not rebuild or modify the underlying proxy unnecessarily. Follow existing naming conventions and architecture.

## 1. Core hierarchy

The application has three levels:

**Recorded Sessions → Events → Formatted Event Structure**

A recorded session is a completed capture of incoming UTF-8 messages received from the server during a user-controlled recording interval.

Each session contains an ordered collection of events. Each event represents one received message at a single point in time, not an activity with a duration.

The left sidebar primarily displays recorded sessions. Selecting a session opens its event collection in the centre panel. Selecting an event displays its structured content in the right panel.

## 2. Recording

- Register a configurable background keyboard shortcut, with `X` merely a placeholder.
- Press once to start a new recorded session; press again to stop and finalise it.
- Recording must operate without requiring the inspector window to be focused.
- Capture only incoming server messages, using the existing proxy's decoded message representation.
- Preserve exact observed reception order using monotonically increasing sequence numbers.
- Record reception timestamps where available.
- Recording must not block, slow down or interfere with proxy traffic.
- Avoid expensive UI updates or formatting on the interception path.

## 3. Session management

Automatically save sessions into a dedicated, configurable local directory.

Each saved session should contain its metadata and complete collection of recorded events.

Provide a left-hand session explorer showing:

- Session creation date and time
- Event count
- Recording start/end timestamps
- Duration as optional metadata
- Selected-session indication

Allow opening previous sessions, deleting sessions with confirmation, opening the storage folder, and exporting individual sessions.

Session storage must preserve the original event data and order. Use an appropriate lossless, versioned format. Prefer atomic finalisation and reasonable crash recovery.

## 4. Event explorer

When a recorded session is selected, load its events into a central chronological list.

Each event row should show:

- Original sequence number
- Reception timestamp
- Message type, such as CRA, ADI, SDI, etc.
- A concise, readable summary derived from the decoded structure

Above the list, automatically generate type-filter buttons based on the types actually present in that session, with their counts.

Filters are multi-select and operate only within the selected recorded session.

For example, selecting CRA and ADI displays only those event types, interleaved in their original reception order.

Filtering must never modify the saved session or renumber events.

Provide optional text search and efficient navigation through larger recordings. Use list virtualisation where necessary.

## 5. Formatted event inspector

This is the most important UI requirement.

**The primary detail view must not be a raw payload dump.**

When an event is selected, render its decoded structure in a clean, readable, hierarchical format.

For example, a CRA event should display the actual CRA structure exposed by the existing decoder, divided into meaningful sections where the underlying data supports them.

Use:

- Label/value fields
- Expandable nested objects
- Collapsible arrays
- Clear indentation
- Readable number and timestamp formatting
- Structured tables where appropriate
- Consistent handling of null, missing or unknown fields

Determine the real message schemas by inspecting the existing proxy and decoding code. Do not invent meanings for unfamiliar fields.

Where known type-specific structures exist, use dedicated formatters. For unknown types, provide a generic recursive structured viewer rather than failing or presenting an unreadable string.

A secondary raw-view option is acceptable for advanced debugging, but formatted presentation must be the default.

## 6. Interface

Use a restrained, polished desktop-style layout:

**Left:** Recorded sessions and session actions.

**Centre:** Selected session's ordered event list, type filters and search.

**Right:** Formatted event details.

Keep panels independently scrollable. Do not create competing automatically scrolling views. Completed sessions should remain stationary during inspection.

Maintain a consistent visual hierarchy, compact spacing and readable typography. Avoid excessive decoration.

## 7. Export

Provide an Export Session action.

Initially support a lossless structured format such as JSON, including metadata, event order, timestamps, type information and complete decoded event content.

Export must respect the distinction between stored data and UI filters: by default, export the complete selected session, not merely currently visible events.

## 8. Implementation principles

- Keep the existing interception mechanism intact.
- Separate capture, persistence, querying/filtering and presentation.
- Use one canonical session representation.
- Avoid redundant copies of large event payloads where practical.
- Keep filtering deterministic and independent of the original data.
- Handle malformed or unknown message types without losing the rest of the recording.
- Design modules to fit the existing project structure rather than imposing arbitrary filenames or unnecessary abstractions.
- Preserve the original session data even if the decoder or presentation logic evolves.

## 9. Expected result

On launching the inspector, I should see my previously recorded sessions in the left sidebar.

Clicking a session loads all events belonging to it. Selecting event-type buttons filters the event list while preserving original reception order.

Clicking any event opens its nicely formatted, type-aware structure in the right-hand panel.

The application should feel like a purpose-built network session explorer, not a general-purpose log viewer or packet dump.

Before implementing, inspect the supplied proxy injection, identify the current interception and decoding points, and adapt this specification to the real structures and APIs. Flag genuine ambiguities rather than guessing message schemas.
