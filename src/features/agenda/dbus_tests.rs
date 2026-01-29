// Integration tests for agenda DBus functionality require a running Evolution Data Server.
//
// The agenda module uses Evolution Data Server's complex D-Bus API:
// - org.gnome.evolution.dataserver.SourceManager for discovering calendar sources
// - org.gnome.evolution.dataserver.Calendar for opening calendars
// - org.gnome.evolution.dataserver.CalendarView for querying events
//
// These interfaces require:
// - Running Evolution Data Server (evolution-data-server package)
// - Configured calendar sources (local or remote)
// - Complex setup of views with SEXP queries
//
// Future work: Add integration tests using mock Evolution Data Server or test fixtures.
//
// Test scenarios to add:
// - discover_calendar_sources() finds local and remote calendars
// - discover_calendar_sources() filters out disabled sources
// - open_calendar() returns valid bus name and calendar path
// - create_view() successfully creates a calendar view with SEXP query
// - start_view() initiates event fetching
// - stop_and_dispose_view() properly cleans up resources
// - listen_view_signals() receives ObjectsAdded signals
// - listen_view_signals() receives ObjectsModified signals
// - listen_view_signals() receives ObjectsRemoved signals
// - parse_ical_component() extracts event properties (summary, start, end, location)
// - Time range queries return only events within specified period
