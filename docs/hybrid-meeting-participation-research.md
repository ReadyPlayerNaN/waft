# Hybrid Meeting Participation: Research & Implementation Guide

**Investigation Date**: 2026-02-04
**Status**: Research Complete - Ready for Implementation
**License**: CC0 (Public Domain)

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Investigation: Current State](#investigation-current-state)
3. [The Standard: RFC 9073](#the-standard-rfc-9073)
4. [Empirical Testing](#empirical-testing)
5. [Platform Survey](#platform-survey)
6. [Opportunity Analysis](#opportunity-analysis)
7. [Implementation Guide](#implementation-guide)
8. [References](#references)

---

## Problem Statement

### User Need

Users want to indicate **how they will attend** hybrid meetings:
- **In-person**: Attending from the meeting room/office
- **Virtual**: Attending remotely via video conference
- **Hybrid**: Can do either

### Real-World Value

This information helps:
- **Meeting organizers**: Plan room capacity and A/V equipment
- **Attendees**: Know who will be physically present
- **Facilities**: Optimize space usage and resources
- **Participants**: Better coordinate hybrid meeting logistics

### Initial Question

> "Is it possible to detect from the calendar if I decided to join virtually or in a meeting room?"

**Answer**: Google Calendar has this feature in the UI since 2021, but it's **not exported** to CalDAV, iCalendar, or any API. This investigation explores why and what we can do about it.

---

## Investigation: Current State

### What Calendar Data Currently Contains

We examined actual calendar events from Evolution Data Server (EDS) connected to Google Calendar via CalDAV. Here's what's available:

#### Standard ATTENDEE Properties (RFC 5545)
```icalendar
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;
 CN=pavel.zak@cookielab.io;X-NUM-GUESTS=0:mailto:pavel.zak@cookielab.io
```

**Available Parameters**:
- `PARTSTAT`: Participation status (ACCEPTED, DECLINED, TENTATIVE, NEEDS-ACTION)
- `CN`: Common name (display name)
- `ROLE`: REQ-PARTICIPANT, OPT-PARTICIPANT, CHAIR
- `CUTYPE`: INDIVIDUAL, GROUP, RESOURCE, ROOM
- `X-NUM-GUESTS`: Google-specific guest count

**Missing**: Any indication of *how* the person will attend (virtual vs in-person)

#### What Google Calendar Provides via CalDAV

```icalendar
BEGIN:VEVENT
UID:team-lunch-001@google.com
SUMMARY:Team lunch
DTSTART;TZID=Europe/Prague:20260204T114500
DTEND;TZID=Europe/Prague:20260204T124500
LOCATION:Pobřežní 46-4-Big - Meeting Room 2 (10)
ORGANIZER;CN=martin@cookielab.io:mailto:martin@cookielab.io
ATTENDEE;CUTYPE=INDIVIDUAL;PARTSTAT=ACCEPTED;CN=pavel.zak@cookielab.io:
 mailto:pavel.zak@cookielab.io
X-GOOGLE-CONFERENCE:https://meet.google.com/phw-qyrw-pcw
END:VEVENT
```

**Key Observations**:
- ✅ Both `LOCATION` (physical) and `X-GOOGLE-CONFERENCE` (virtual) present → Hybrid meeting
- ❌ No indication of how individual attendees plan to attend
- ✅ `X-GOOGLE-CONFERENCE` is preserved through CalDAV
- ❌ No `X-GOOGLE-RESPONSE-METHOD` or similar parameter

### Google Calendar's "Join Virtually" Feature

**Launch**: July 2021

**User Experience**:
When accepting a meeting invitation in Google Calendar, users see:
- "Yes"
- "Yes, in a meeting room"
- "Yes, join virtually"

Both organizers and attendees can see how each person plans to attend **within Google Calendar only**.

**The Problem**: This information is stored in Google's internal database and is **not exported** via:
- ❌ CalDAV/iCalendar
- ❌ Google Calendar API v3
- ❌ Any other mechanism

**Why This Matters**:
- Cross-platform calendar clients (Evolution, Thunderbird, Apple Calendar) can't see this information
- The feature creates a walled garden
- Users dependent on Google Calendar web/mobile apps

**Source**: [Google Workspace Updates: Join meeting virtually or in person (July 2021)](https://workspaceupdates.googleblog.com/2021/07/join-meeting-virtually-or-in-person-google-calendar.html)

---

## The Standard: RFC 9073

### Overview

**RFC 9073**: "Event Publishing Extensions to iCalendar"
**Published**: August 2021 (same time as Google's feature!)
**Status**: Proposed Standard
**Purpose**: Extend iCalendar with components useful for event publishing and social networking

**Official Document**: [RFC 9073](https://www.rfc-editor.org/rfc/rfc9073.html)

### The PARTICIPANT Component

RFC 9073 introduces the `PARTICIPANT` component, designed specifically to solve the hybrid meeting problem.

**From the RFC**:
> "For a meeting, the room size and equipment needed depends on the number of attendees actually in the room. The current ATTENDEE property does not allow for the addition of such metadata. The PARTICIPANT component allows attendees to specify their location."

### PARTICIPANT Structure

**Required Properties**:
- `UID`: Unique identifier
- `PARTICIPANT-TYPE`: Role type (ACTIVE, INACTIVE, SPONSOR, etc.)

**Optional Properties** (selection):
- `CALENDAR-ADDRESS`: Links to ATTENDEE via email
- `LOCATION`: Where the participant will be
- `DESCRIPTION`: Details about participation
- `COMMENT`: Additional notes
- Can contain nested `VLOCATION` components

### Example: Hybrid Meeting with PARTICIPANT

```icalendar
BEGIN:VEVENT
UID:team-meeting-2026-02-11@example.com
DTSTAMP:20260204T120000Z
DTSTART:20260211T140000Z
DTEND:20260211T150000Z
SUMMARY:Weekly Team Sync
LOCATION:Conference Room A, Building 2
ORGANIZER;CN=Manager:mailto:manager@example.com

ATTENDEE;CN=Pavel;PARTSTAT=ACCEPTED:mailto:pavel@example.com
BEGIN:PARTICIPANT
UID:part-pavel-001
CALENDAR-ADDRESS:mailto:pavel@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Conference Room A, Building 2
DESCRIPTION:Attending in person
END:PARTICIPANT

ATTENDEE;CN=Jana;PARTSTAT=ACCEPTED:mailto:jana@example.com
BEGIN:PARTICIPANT
UID:part-jana-001
CALENDAR-ADDRESS:mailto:jana@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Conference Room A, Building 2
DESCRIPTION:Attending in person
END:PARTICIPANT

ATTENDEE;CN=Remote Worker;PARTSTAT=ACCEPTED:mailto:remote@example.com
BEGIN:PARTICIPANT
UID:part-remote-001
CALENDAR-ADDRESS:mailto:remote@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Remote via Zoom
DESCRIPTION:Joining virtually from home
END:PARTICIPANT

X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij
END:VEVENT
```

**What This Enables**:
- Organizer sees 2 people in-room, 1 virtual
- Can plan A/V setup accordingly
- Room capacity planning
- Hybrid meeting coordination

### Why RFC 9073 Is The Right Solution

✅ **Standards-Based**: IETF Proposed Standard, open specification
✅ **Backward Compatible**: Clients ignore unknown components gracefully
✅ **Vendor Neutral**: No lock-in, works across platforms
✅ **Extensible**: Can add custom properties as needed
✅ **Per-Attendee**: Each participant specifies their own location
✅ **Works Today**: No breaking changes to existing calendars

---

## Empirical Testing

We conducted real-world tests to validate whether RFC 9073 PARTICIPANT components work with current calendar systems.

### Test Methodology

**Date**: 2026-02-04
**System**: Evolution Data Server (EDS) 3.x via D-Bus API
**Calendars Tested**:
1. Local calendar (`system-calendar`)
2. Google Calendar via CalDAV (`pavel.zak@cookielab.io`)

**Method**: Create events with PARTICIPANT components, retrieve them back, check if preserved

### Test 1: Local Calendar (Evolution Data Server)

**Input Event**:
```icalendar
BEGIN:VEVENT
UID:test-participant-1770223596
DTSTAMP:20260204T164636Z
DTSTART:20260211T164636Z
DTEND:20260211T174636Z
SUMMARY:Test RFC 9073 PARTICIPANT
DESCRIPTION:Testing if PARTICIPANT components work
ORGANIZER:mailto:pavel.zak@cookielab.io
ATTENDEE;CN=Pavel Zak;PARTSTAT=ACCEPTED:mailto:pavel.zak@cookielab.io
BEGIN:PARTICIPANT
UID:participant-1770223596
CALENDAR-ADDRESS:mailto:pavel.zak@cookielab.io
PARTICIPANT-TYPE:ACTIVE
LOCATION:Meeting Room 2 - In Person
DESCRIPTION:Attending in person
END:PARTICIPANT
END:VEVENT
```

**Result**: ✅ Event **accepted** and **created**

**Retrieved Event**:
```icalendar
BEGIN:VEVENT
UID:test-participant-1770223596
DTSTAMP:20260204T164636Z
DTSTART:20260211T164636Z
DTEND:20260211T174636Z
SUMMARY:Test RFC 9073 PARTICIPANT
DESCRIPTION:Testing if PARTICIPANT components work
ORGANIZER:mailto:pavel.zak@cookielab.io
ATTENDEE;CN=Pavel Zak;PARTSTAT=ACCEPTED:mailto:pavel.zak@cookielab.io
CREATED:20260204T164637Z
LAST-MODIFIED:20260204T164637Z
END:VEVENT
```

**Finding**: ❌ **PARTICIPANT component was stripped**

**Conclusion**: Evolution Data Server does not support RFC 9073 PARTICIPANT components. It follows the iCalendar spec by ignoring unknown components, but does not preserve or understand them.

### Test 2: Google Calendar via CalDAV

**Input Event**:
```icalendar
BEGIN:VEVENT
UID:test-google-participant-1770224631
DTSTAMP:20260204T170351Z
DTSTART:20260211T170351Z
DTEND:20260211T180351Z
SUMMARY:RFC 9073 PARTICIPANT Test on Google
DESCRIPTION:Testing if Google Calendar preserves PARTICIPANT via CalDAV
ORGANIZER:mailto:pavel.zak@cookielab.io
ATTENDEE;CN=Pavel Zak;PARTSTAT=ACCEPTED:mailto:pavel.zak@cookielab.io
BEGIN:PARTICIPANT
UID:participant-google-1770224631
CALENDAR-ADDRESS:mailto:pavel.zak@cookielab.io
PARTICIPANT-TYPE:ACTIVE
LOCATION:Pobřežní 46 - Meeting Room
DESCRIPTION:Attending in person from office
COMMENT:In-person attendance
END:PARTICIPANT
X-GOOGLE-CONFERENCE:https://meet.google.com/test-link
END:VEVENT
```

**Result**: ✅ Event **accepted** and **synced to Google Calendar**

**Retrieved Event** (excerpt):
```icalendar
BEGIN:VEVENT
DTSTART:20260211T170351Z
DTEND:20260211T180351Z
UID:test-google-participant-1770224631
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;
 CN=pavel.zak@cookielab.io;X-NUM-GUESTS=0:mailto:pavel.zak@cookielab.io
X-GOOGLE-CONFERENCE:https://meet.google.com/test-link
DESCRIPTION:Testing if Google Calendar preserves PARTICIPANT via CalDAV
 [... Google Meet boilerplate auto-added ...]
SUMMARY:RFC 9073 PARTICIPANT Test on Google
STATUS:CONFIRMED
END:VEVENT
```

**Findings**:
- ✅ Event synced successfully via CalDAV
- ✅ `X-GOOGLE-CONFERENCE` **preserved**
- ✅ Google Meet boilerplate **auto-added** to description
- ❌ **PARTICIPANT component stripped**

**Conclusion**: Google Calendar does not support RFC 9073 PARTICIPANT components via CalDAV. It preserves its own `X-GOOGLE-*` extensions but not the standard PARTICIPANT component.

### Test Results Summary

| Calendar System | Accepts Event | Creates Event | Preserves PARTICIPANT | RFC 9073 Support |
|----------------|---------------|---------------|----------------------|------------------|
| EDS Local      | ✅            | ✅            | ❌                   | ❌               |
| Google CalDAV  | ✅            | ✅            | ❌                   | ❌               |

**Key Insight**: Both systems correctly implement graceful degradation (ignore unknown components per RFC 5545), but neither implements RFC 9073 support.

**Implication**: We can safely use PARTICIPANT components without breaking existing calendar systems - they'll just ignore them.

---

## Platform Survey

### Summary Table

| Platform | Has Feature | Year | API/Export | Implementation | Status |
|----------|-------------|------|------------|----------------|--------|
| **Google Calendar** | ✅ Yes | 2021 | ❌ No | UI-only, proprietary | Walled garden |
| **Microsoft Outlook** | ✅ Yes | 2024 | ❓ Unknown | UI-only | Likely walled |
| **Apple Calendar** | ❌ No | - | - | None found | No feature |
| **Nextcloud** | ❌ No | - | - | Tracks RFC 9073 | Not implemented |
| **Calendly** | ✅ Yes | 2025 | ✅ Yes | SaaS-specific | Not standard |
| **RFC 9073** | ✅ Standard | 2021 | ✅ Yes | Open standard | **Zero adoption** |

### 1. Google Calendar

**Feature**: "Join Virtually or In-Person" RSVP
**Launch**: July 2021
**Availability**: Web, Mobile, Gmail

**How It Works**:
- Dropdown when accepting invitations:
  - "Yes"
  - "Yes, in a meeting room"
  - "Yes, join virtually"
- Visible to organizers and attendees

**Technical Reality**:
- ❌ Not exported via CalDAV (confirmed by testing)
- ❌ Not in Google Calendar API v3
- ❌ Not visible to non-Google calendar clients
- ✅ `X-GOOGLE-CONFERENCE` links ARE exported

**Attendee Format** (what's actually exported):
```icalendar
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;
 CN=user@example.com;X-NUM-GUESTS=0:mailto:user@example.com
```
No `X-RESPONSE-METHOD` or similar parameter exists.

**Sources**:
- [Google Workspace Updates (July 2021)](https://workspaceupdates.googleblog.com/2021/07/join-meeting-virtually-or-in-person-google-calendar.html)
- [Google Calendar API Reference](https://developers.google.com/workspace/calendar/api/v3/reference/events)

### 2. Microsoft Outlook

**Feature**: "In-Person Events and Hybrid RSVPing"
**Launch**: March-April 2024
**Availability**: Outlook on the web, Outlook (new) for Windows

**How It Works**:
- Organizers mark events as "in-person"
- When responding, attendees choose:
  - "Yes, in-person"
  - "Yes, virtually"
  - "Yes" (no mode specified)
- Organizers track responses in tracking pane

**Technical Reality**:
- ❓ Microsoft Graph API support: **UNKNOWN**
  - Standard `attendee` resource has no participation method field
  - `responseStatus` only shows accepted/declined/tentative
- ❓ Exchange/CalDAV export: **UNKNOWN**
- Feature may be Outlook UI-only

**API Schema** (standard, no hybrid info):
```json
{
  "emailAddress": { "address": "user@example.com" },
  "type": "required",
  "status": {
    "response": "accepted",
    "time": "2024-03-15T10:00:00Z"
  }
}
```

**Sources**:
- [Microsoft TechCommunity: Improved hybrid meeting experience](https://techcommunity.microsoft.com/blog/outlook/improved-hybrid-meeting-experience-in-outlook/4061045)
- [Microsoft Graph API: attendee resource](https://learn.microsoft.com/en-us/graph/api/resources/attendee?view=graph-rest-1.0)

### 3. Apple Calendar

**Feature Status**: ❌ **NOT FOUND**

**Findings**:
- No hybrid meeting feature found
- No in-person vs virtual attendance indication
- Limited video conferencing: FaceTime only
- Cannot add Zoom/Meet/Teams from Calendar UI

**CalDAV Support**:
- ✅ Supports CalDAV sync
- ✅ Send/receive invitations
- ⚠️ Known issue: CalDAV users with write access on shared calendars cannot add attendees

**Workarounds**:
- Add Google Calendar accounts to sync meetings
- Manual links for Zoom/Teams
- Third-party automation tools

**Sources**:
- [Apple: Invite people to events](https://support.apple.com/guide/calendar/invite-people-to-events-icl1016/11.0/mac/13.0)
- [Apple: Reply to invitations](https://support.apple.com/guide/calendar/reply-to-invitations-icl1019/mac)

### 4. Nextcloud Calendar

**Feature Status**: ❌ **NOT IMPLEMENTED** (but aware of standard)

**RFC 9073 Awareness**:
- ✅ Listed in [Developer Resources wiki](https://github.com/nextcloud/calendar/wiki/Developer-Resources)
- ❌ No actual implementation found

**Backend**: Uses [Sabre/DAV](https://sabre.io/) for CalDAV
- Sabre/DAV's [Standards Support](https://sabre.io/dav/standards-support/) doesn't list RFC 9073

**Current Features**:
- ✅ Attendees with email confirmations
- ✅ RSVP (accept/decline)
- ✅ Resource and room booking
- ❌ No virtual vs in-person tracking

**2025 Updates**:
- "Meetings proposal" for finding time slots
- No hybrid meeting features

**Sources**:
- [Nextcloud Calendar Admin Manual](https://docs.nextcloud.com/server/stable/admin_manual/groupware/calendar.html)
- [Nextcloud Calendar 2025 updates](https://nextcloud.com/blog/reclaim-your-schedule-and-your-privacy-with-nextcloud-calendar-discover-the-2025-updates/)

### 5. Calendly

**Feature**: "Location Options"
**Launch**: Early 2025 (general availability)

**How It Works**:
- Hosts offer multiple location choices:
  - "Zoom OR Teams"
  - "Virtual call OR office meeting"
- **Limitation**: One-on-one meetings only
- Not supported for group events

**API Support**: ✅ **Yes** (Scheduling API, October 2025)
```json
{
  "location": {
    "kind": "zoom",
    "location": "https://zoom.us/j/123456789"
  }
}
```

**Workarounds**:
- Separate events for in-person vs virtual
- Routing forms to ask preference

**Note**: SaaS-specific, not iCalendar standard-based

**Sources**:
- [Calendly: Hybrid Location Option](https://community.calendly.com/how-do-i-40/hybrid-location-option-3604)
- [Calendly: Location Options Product News](https://community.calendly.com/product-updates/product-news-start-meetings-off-right-by-offering-location-options-2911)

### 6. RFC 9073 PARTICIPANT

**Status**: Published standard, **ZERO adoption**

**IETF Activity**:
- Base RFC 9073 stable (Proposed Standard, August 2021)
- November 2024: [Discussion on iTip with participants](https://datatracker.ietf.org/meeting/121/materials/slides-121-calext-itip-participants-00)
- Draft [draft-ietf-calext-itip-participants](https://datatracker.ietf.org/doc/draft-ietf-calext-itip-participants/) expired June 2025

**Known Implementations**: **NONE**
- ❌ Evolution Data Server: No
- ❌ Google Calendar: No
- ❌ Microsoft Outlook: Unknown, likely no
- ❌ Apple Calendar: Unknown, likely no
- ❌ Nextcloud: No (tracks but not implemented)
- ❌ Thunderbird: Unknown, likely no
- ❌ Sabre/DAV: No

**Why Zero Adoption?**

1. **Network Effect Problem**: Each vendor waiting for others
2. **UI-First Culture**: Product teams ship UI features, not standards
3. **Lock-in Incentives**: Platform-exclusive features retain users
4. **Incomplete Specification**: iTip scheduling support still draft
5. **Resource Constraints**: Standards implementation requires investment

**Sources**:
- [RFC 9073](https://www.rfc-editor.org/rfc/rfc9073.html)
- [IETF calext Working Group](https://datatracker.ietf.org/group/calext/about/)

---

## Opportunity Analysis

### Why Open Source Can Lead

**The Deadlock**:
- Google: "Why implement if Apple won't?"
- Apple: "Why implement if Microsoft won't?"
- Microsoft: "Why implement if Google won't?"
- **Nobody moves first**

**Open Source Advantages**:
1. ✅ **No Lock-in Incentive**: We benefit from interoperability
2. ✅ **Community Coordination**: Can sync across projects
3. ✅ **User-Driven**: Prioritize needs over business strategy
4. ✅ **Standards Alignment**: Philosophical commitment to open standards
5. ✅ **Proof of Concept**: Can demonstrate value before vendor adoption

**Historical Precedents**:
- **CalDAV**: Apple started, open source (DAViCal, Radicale) drove adoption
- **WebDAV**: Open source servers proved viability
- **ActivityPub**: Mastodon led federated social networking
- **Matrix**: Open source drove federated messaging

### Target Projects for Coordination

| Project | Language | Impact | Priority | Rationale |
|---------|----------|--------|----------|-----------|
| **Evolution Data Server** | C | High | 🔥 Critical | GNOME default, backend for many clients |
| **Nextcloud Calendar** | PHP | High | 🔥 Critical | Already tracks RFC 9073, large user base |
| **Thunderbird** | JavaScript | High | ⭐ High | Mozilla backing, cross-platform reach |
| **Sabre/DAV** | PHP | High | 🔥 Critical | Backend for Nextcloud and many others |
| **DAViCal** | PHP | Medium | ⭐ High | CalDAV server, standards-focused |
| **Radicale** | Python | Medium | ⭐ High | Lightweight CalDAV server |
| **Baikal** | PHP | Medium | ⚡ Medium | Uses Sabre/DAV |

### The Strategy: Reference Implementation First

**Phase 1: Proof of Concept** (1-2 months)
- Implement read-only PARTICIPANT parsing in Sacrebleui
- Display "You: In-person" / "You: Virtual" in UI
- Document user experience
- Blog post: "The Missing Calendar Standard"

**Phase 2: Server Support** (3-6 months)
- File feature requests with:
  - Evolution Data Server
  - Nextcloud Calendar
  - Sabre/DAV
- Contribute patches where possible
- Create interoperability test suite

**Phase 3: Cross-Project Coordination** (6-12 months)
- Form "RFC 9073 Coalition" working group
- Get 3-5 projects to implement
- Shared documentation and test events
- Coordinate release timeline

**Phase 4: Create Pressure** (12+ months)
- Document working implementations
- User testimonials and case studies
- Submit feature requests to Google, Microsoft, Apple
- Present at FOSDEM, conferences
- Standards win when they're **used**, not just published

### Why This Can Work

**The pieces are in place**:
- ✅ Standard exists (RFC 9073, 2021)
- ✅ Problem is real (hybrid work is permanent)
- ✅ Vendors have shown interest (Google, Microsoft built features)
- ✅ Open source can coordinate (no competitive conflicts)
- ✅ Technology works (our testing proves graceful degradation)

**We just need to use it.**

---

## Implementation Guide

### For Calendar Clients (like Sacrebleui)

#### Phase 1: Read-Only Display

**Goal**: Parse PARTICIPANT components and display participation method

**Data Structures**:
```rust
/// Participant information from PARTICIPANT component (RFC 9073)
#[derive(Debug, Clone)]
pub struct Participant {
    pub uid: String,
    pub calendar_address: Option<String>,
    pub participant_type: String, // ACTIVE, INACTIVE, etc.
    pub location: Option<String>,
    pub description: Option<String>,
    pub comment: Option<String>,
}

/// User's participation method
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParticipationMethod {
    InPerson,     // Physical location
    Virtual,      // Remote via video conference
    Hybrid,       // Can do either
    Unknown,      // Cannot determine
}
```

**Parsing Logic**:
```rust
// In parse_vevent(), add PARTICIPANT parsing
fn parse_vevent(ical: &str) -> Option<AgendaEvent> {
    // ... existing parsing ...

    let mut participants: Vec<Participant> = Vec::new();
    let mut in_participant = false;
    let mut current_participant = String::new();

    for line in lines {
        if line.starts_with("BEGIN:PARTICIPANT") {
            in_participant = true;
            current_participant.clear();
            current_participant.push_str(line);
            current_participant.push_str("\r\n");
        } else if line.starts_with("END:PARTICIPANT") {
            current_participant.push_str(line);
            if let Some(p) = parse_participant(&current_participant) {
                participants.push(p);
            }
            in_participant = false;
        } else if in_participant {
            current_participant.push_str(line);
            current_participant.push_str("\r\n");
        }
    }

    // ... rest of event parsing ...
}

fn parse_participant(component: &str) -> Option<Participant> {
    let mut uid = None;
    let mut calendar_address = None;
    let mut participant_type = None;
    let mut location = None;
    let mut description = None;

    for line in component.lines() {
        if line.starts_with("UID:") {
            uid = Some(line[4..].to_string());
        } else if line.starts_with("CALENDAR-ADDRESS:") {
            calendar_address = Some(line[17..].trim().to_string());
        } else if line.starts_with("PARTICIPANT-TYPE:") {
            participant_type = Some(line[17..].to_string());
        } else if line.starts_with("LOCATION:") {
            location = Some(unescape_ical_text(&line[9..]));
        } else if line.starts_with("DESCRIPTION:") {
            description = Some(unescape_ical_text(&line[12..]));
        }
    }

    Some(Participant {
        uid: uid?,
        calendar_address,
        participant_type: participant_type.unwrap_or_else(|| "ACTIVE".to_string()),
        location,
        description,
        comment: None,
    })
}
```

**Detection Logic**:
```rust
fn determine_participation_method(
    event: &AgendaEvent,
    user_email: &str,
) -> ParticipationMethod {
    // Find user's PARTICIPANT component
    if let Some(participant) = event.participants.iter()
        .find(|p| p.calendar_address.as_ref()
            .map_or(false, |addr| addr.contains(user_email)))
    {
        if let Some(location) = &participant.location {
            // Check location text for indicators
            if is_physical_location(location) {
                return ParticipationMethod::InPerson;
            }
            if is_virtual_location(location) {
                return ParticipationMethod::Virtual;
            }
        }
    }

    // Fallback: Heuristics based on event properties
    let has_conference = event.meeting_links.is_some();
    let has_physical_location = event.location.as_ref()
        .map_or(false, |loc| is_physical_location(loc));

    match (has_conference, has_physical_location) {
        (true, true) => ParticipationMethod::Hybrid,
        (true, false) => ParticipationMethod::Virtual,
        (false, true) => ParticipationMethod::InPerson,
        (false, false) => ParticipationMethod::Unknown,
    }
}

fn is_physical_location(location: &str) -> bool {
    let lower = location.to_lowercase();
    lower.contains("room ")
        || lower.contains("building ")
        || lower.contains("floor ")
        || lower.contains("office ")
        || lower.contains("conference ")
        || (!lower.contains("http") && !lower.contains("remote"))
}

fn is_virtual_location(location: &str) -> bool {
    let lower = location.to_lowercase();
    lower.contains("remote")
        || lower.contains("virtual")
        || lower.contains("online")
        || lower.contains("zoom")
        || lower.contains("meet")
        || lower.contains("teams")
        || lower.starts_with("http")
}
```

**UI Display**:
```rust
// In widget.rs, build_event_card()
fn build_event_card(event: &AgendaEvent, user_email: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);

    // ... existing time and summary ...

    // Add participation indicator
    let participation = determine_participation_method(event, user_email);
    if participation != ParticipationMethod::Unknown {
        let indicator = build_participation_indicator(&participation);
        card.append(&indicator);
    }

    // ... rest of card ...
    card
}

fn build_participation_indicator(method: &ParticipationMethod) -> gtk::Box {
    let (icon, text, css_class) = match method {
        ParticipationMethod::InPerson =>
            ("location-services-active-symbolic", "You: In-person", "participation-in-person"),
        ParticipationMethod::Virtual =>
            ("video-display-symbolic", "You: Virtual", "participation-virtual"),
        ParticipationMethod::Hybrid =>
            ("network-workgroup-symbolic", "You: Hybrid", "participation-hybrid"),
        ParticipationMethod::Unknown => return gtk::Box::new(gtk::Orientation::Horizontal, 0),
    };

    let box_widget = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .css_classes([css_class, "participation-indicator"])
        .build();

    let icon = gtk::Image::builder()
        .icon_name(icon)
        .icon_size(gtk::IconSize::Small)
        .build();

    let label = gtk::Label::builder()
        .label(text)
        .css_classes(["caption"])
        .build();

    box_widget.append(&icon);
    box_widget.append(&label);
    box_widget
}
```

**CSS Styling**:
```css
.participation-indicator {
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 0.9em;
}

.participation-in-person {
    background-color: alpha(@green_3, 0.15);
    color: @green_3;
}

.participation-virtual {
    background-color: alpha(@blue_3, 0.15);
    color: @blue_3;
}

.participation-hybrid {
    background-color: alpha(@orange_3, 0.15);
    color: @orange_3;
}
```

#### Phase 2: Event Creation (Future)

**Requires**: Evolution Data Server support for PARTICIPANT preservation

**Goal**: Create events with PARTICIPANT components

**Example**:
```rust
fn create_event_with_participation(
    summary: &str,
    start: DateTime,
    end: DateTime,
    location: &str,
    user_email: &str,
    participation: ParticipationMethod,
) -> String {
    let uid = generate_uid();
    let participant_uid = generate_uid();

    let participant_location = match participation {
        ParticipationMethod::InPerson => location.to_string(),
        ParticipationMethod::Virtual => "Remote via video conference".to_string(),
        ParticipationMethod::Hybrid => format!("{} or remote", location),
        ParticipationMethod::Unknown => "TBD".to_string(),
    };

    format!(
        "BEGIN:VEVENT\r\n\
         UID:{}\r\n\
         DTSTAMP:{}\r\n\
         DTSTART:{}\r\n\
         DTEND:{}\r\n\
         SUMMARY:{}\r\n\
         LOCATION:{}\r\n\
         ORGANIZER:mailto:{}\r\n\
         ATTENDEE;PARTSTAT=NEEDS-ACTION:mailto:{}\r\n\
         BEGIN:PARTICIPANT\r\n\
         UID:{}\r\n\
         CALENDAR-ADDRESS:mailto:{}\r\n\
         PARTICIPANT-TYPE:ACTIVE\r\n\
         LOCATION:{}\r\n\
         DESCRIPTION:Participation method set by user\r\n\
         END:PARTICIPANT\r\n\
         END:VEVENT\r\n",
        uid,
        now_timestamp(),
        format_datetime(start),
        format_datetime(end),
        escape_ical_text(summary),
        escape_ical_text(location),
        user_email,
        user_email,
        participant_uid,
        user_email,
        escape_ical_text(&participant_location)
    )
}
```

### For Calendar Servers (like Evolution Data Server)

#### Minimal Implementation

**Goal**: Preserve PARTICIPANT components without semantic understanding

**Changes Needed**:
1. **Parser**: Recognize `BEGIN:PARTICIPANT` / `END:PARTICIPANT`
2. **Storage**: Store as sub-component of VEVENT
3. **Serialization**: Include PARTICIPANT when outputting iCalendar
4. **No semantic understanding required**: Just pass-through

**Example** (pseudo-code):
```c
// In libecal/e-cal-component.c or similar

typedef struct {
    gchar *uid;
    gchar *calendar_address;
    gchar *participant_type;
    gchar *location;
    gchar *description;
    // ... other properties ...
} ECalComponentParticipant;

// Add to ECalComponent
GSList *participants; /* list of ECalComponentParticipant */

// Parser: When encountering BEGIN:PARTICIPANT
if (g_str_has_prefix(line, "BEGIN:PARTICIPANT")) {
    ECalComponentParticipant *participant = parse_participant_component(lines);
    component->participants = g_slist_append(component->participants, participant);
}

// Serializer: When outputting VEVENT
for (GSList *l = component->participants; l != NULL; l = l->next) {
    ECalComponentParticipant *p = l->data;
    g_string_append(ical_str, "BEGIN:PARTICIPANT\r\n");
    g_string_append_printf(ical_str, "UID:%s\r\n", p->uid);
    if (p->calendar_address)
        g_string_append_printf(ical_str, "CALENDAR-ADDRESS:%s\r\n", p->calendar_address);
    // ... output other properties ...
    g_string_append(ical_str, "END:PARTICIPANT\r\n");
}
```

### Test Events for Validation

**All-Virtual Meeting**:
```icalendar
BEGIN:VEVENT
UID:test-virtual@example.com
SUMMARY:Daily Standup
DTSTART:20260212T090000Z
DTEND:20260212T091500Z
ATTENDEE:mailto:team1@example.com
BEGIN:PARTICIPANT
UID:part-1
CALENDAR-ADDRESS:mailto:team1@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Remote
END:PARTICIPANT
X-GOOGLE-CONFERENCE:https://meet.google.com/abc-def-ghi
END:VEVENT
```

**Hybrid Meeting**:
```icalendar
BEGIN:VEVENT
UID:test-hybrid@example.com
SUMMARY:Product Review
DTSTART:20260213T140000Z
DTEND:20260213T150000Z
LOCATION:Conference Room A
ATTENDEE:mailto:user1@example.com
BEGIN:PARTICIPANT
UID:part-user1
CALENDAR-ADDRESS:mailto:user1@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Conference Room A
END:PARTICIPANT
ATTENDEE:mailto:user2@example.com
BEGIN:PARTICIPANT
UID:part-user2
CALENDAR-ADDRESS:mailto:user2@example.com
PARTICIPANT-TYPE:ACTIVE
LOCATION:Remote via Zoom
END:PARTICIPANT
X-GOOGLE-CONFERENCE:https://zoom.us/j/123456789
END:VEVENT
```

---

## References

### RFC Standards

- [RFC 5545: iCalendar Core Specification](https://www.rfc-editor.org/rfc/rfc5545)
- [RFC 7986: New Properties for iCalendar](https://www.rfc-editor.org/rfc/rfc7986.html)
- [RFC 9073: Event Publishing Extensions to iCalendar](https://www.rfc-editor.org/rfc/rfc9073.html)
- [RFC 4791: CalDAV](https://datatracker.ietf.org/doc/html/rfc4791)
- [RFC 6638: Scheduling Extensions to CalDAV](https://www.rfc-editor.org/rfc/rfc6638.html)

### IETF Working Groups

- [Calendaring Extensions (calext)](https://datatracker.ietf.org/group/calext/about/)
- [iTip with participants - IETF 121 presentation](https://datatracker.ietf.org/meeting/121/materials/slides-121-calext-itip-participants-00)
- [draft-ietf-calext-itip-participants](https://datatracker.ietf.org/doc/draft-ietf-calext-itip-participants/)

### Vendor Documentation

- [Google Calendar API: Events](https://developers.google.com/workspace/calendar/api/v3/reference/events)
- [Google CalDAV API Guide](https://developers.google.com/calendar/caldav/v2/guide)
- [Microsoft Graph API: attendee resource](https://learn.microsoft.com/en-us/graph/api/resources/attendee?view=graph-rest-1.0)
- [Apple Calendar: CalDAV configuration](https://support.apple.com/guide/deployment/calendar-declarative-configuration-depf0ad6bc01/web)

### Vendor Announcements

- [Google: Join meeting virtually or in person (July 2021)](https://workspaceupdates.googleblog.com/2021/07/join-meeting-virtually-or-in-person-google-calendar.html)
- [Microsoft: Improved hybrid meeting experience (2024)](https://techcommunity.microsoft.com/blog/outlook/improved-hybrid-meeting-experience-in-outlook/4061045)
- [Google Calendar invites: physical or virtual attendance](https://9to5google.com/2021/11/15/google-calendar-invite-attendance/)

### Open Source Projects

- [Evolution Data Server (GitLab)](https://gitlab.gnome.org/GNOME/evolution-data-server)
- [Nextcloud Calendar (GitHub)](https://github.com/nextcloud/calendar)
- [Nextcloud Calendar Developer Resources](https://github.com/nextcloud/calendar/wiki/Developer-Resources)
- [Sabre/DAV](https://sabre.io/)
- [Sabre/DAV Standards Support](https://sabre.io/dav/standards-support/)
- [DAViCal](https://www.davical.org/)
- [Radicale](https://radicale.org/)

### Community & Standards Organizations

- [CalConnect](https://www.calconnect.org/)
- [GNOME Discourse](https://discourse.gnome.org/)
- [Nextcloud Community](https://help.nextcloud.com/)
- [iCalendar.org](https://icalendar.org/)

### Additional Resources

- [Calendly: Hybrid Location Option](https://community.calendly.com/how-do-i-40/hybrid-location-option-3604)
- [Calendly: Location Options announcement](https://community.calendly.com/product-updates/product-news-start-meetings-off-right-by-offering-location-options-2911)
- [Aurinko: CalDAV Apple Calendar Integration](https://www.aurinko.io/blog/caldav-apple-calendar-integration/)

---

## Next Actions

### For Sacrebleui

**Week 1**:
- [ ] Implement PARTICIPANT parsing in `values.rs`
- [ ] Add `ParticipationMethod` detection logic
- [ ] Create UI indicator component in `widget.rs`
- [ ] Test with local calendar

**Month 1**:
- [ ] File feature request with GNOME Evolution Data Server
- [ ] Create test .ics files with PARTICIPANT components
- [ ] Document user experience
- [ ] Write blog post: "The Missing Calendar Standard"

**Month 2-3**:
- [ ] Reach out to Nextcloud Calendar team
- [ ] Contact Sabre/DAV maintainers
- [ ] Present to GNOME Discourse community
- [ ] Create RFC 9073 advocacy materials

### For Open Source Community

**Coordination Needed**:
1. Form "RFC 9073 Coalition" working group
2. Get commitments from 3-5 major projects
3. Create shared test suite
4. Coordinate announcement timeline

**Advocacy Needed**:
1. Blog posts from multiple projects
2. Presentation at FOSDEM 2027
3. Social media campaign
4. Feature requests to Google, Microsoft, Apple

---

## Conclusion

### What We Found

1. **The Problem Is Real**: Hybrid work is permanent, users need this feature
2. **The Standard Exists**: RFC 9073 PARTICIPANT (August 2021) solves it perfectly
3. **Vendors Built Features**: Google (2021) and Microsoft (2024) recognized the need
4. **But Created Silos**: No API/export, proprietary implementations
5. **Zero Standard Adoption**: Nobody implements RFC 9073
6. **Open Source Can Lead**: No lock-in incentives, can coordinate

### Why This Matters

**Calendar interoperability is broken.** Vendors compete on features, users lose on portability. Open standards exist but go unused. Someone needs to go first.

**This is open source's chance to lead.**

### The Path Forward

1. ✅ **Implement in Sacrebleui** (proof of concept)
2. 🔄 **Push to Evolution Data Server** (backend support)
3. 🤝 **Coordinate with Nextcloud** (another implementation)
4. 📢 **Advocate publicly** (blog, conferences, social media)
5. 💪 **Create pressure on vendors** (user demand from working implementations)

**Standards win when they're used, not just published.**

---

**Document Status**: Complete
**Last Updated**: 2026-02-04
**Next Review**: After Phase 1 implementation

**License**: CC0 (Public Domain) - Use this research freely to advance open calendar standards.
