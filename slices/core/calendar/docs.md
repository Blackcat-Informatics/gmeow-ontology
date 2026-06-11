# Calendar and Scheduling Mapping

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

This document describes the GMEOW calendar and scheduling slice (#62): the mapping
between GMEOW's canonical scheduling layer and external calendar vocabularies —
iCalendar (RFC 5545/5546/6047), jCal (RFC 7265), xCal (RFC 6321), CalDAV (RFC
4791/6638/7809), schema.org `Schedule`/`EventSeries`, OWL-Time, and Wikidata.

## Design Principles

1. **Schedule ≠ occurrence.** An `EventSchedule` (rule + anchor + time zone)
   *generates* `Event` occurrences; it is not itself the occurrence. Reuse
   `EventSeries`/`RecurrenceRule`; add occurrence-generation + exceptions.
2. **Reify the social acts as relators** (the `Participation` idiom):
   `EventInvitation` (event × invitee × status), `Availability` (agent × interval
   × status) — never flat properties.
3. **Open value vocabularies, not subclasses:** `invitationStatus`/`rsvpStatus`
   (needs-action / accepted / declined / tentative — iTIP PARTSTAT),
   `availabilityStatus` (free / busy / tentative / out-of-office),
   `reminderAction` (display / email / audio), `taskStatus` (not-started /
   in-progress / completed / cancelled).
4. **Cancelled ≠ deleted (Principle 10).** A cancelled/rescheduled occurrence is
   `displayable false` + a `ScheduleException`, retained, never erased.
5. **Contested times coexist (#43).** A disputed time/date is standpoint-indexed
   via `accordingTo`; no single winner.

## Core Classes

| GMEOW | gUFO grounding | Description |
|-------|---------------|-------------|
| `Calendar` | `InformationObject` | CalDAV-style collection of events |
| `EventSchedule` | `Relator` | Recurrence rule + template event + time zone |
| `ScheduleException` | `Relator` | Cancellation or rescheduling exception |
| `EventInvitation` | `Agreement` + `Relator` | Invitation ≈ agreement to attend |
| `Availability` | `TimeScopedRelation` | Agent's free/busy/tentative/ooo slot |
| `Reminder` | `Entity` | Trigger + action attached to an event |
| `Task` | `Event` | To-do with due date, status, priority |
| `TimeZone` | `Entity` | IANA tzid (e.g. America/Toronto) |

## Value Vocabularies (individuals, never subclasses)

| Vocabulary | Seed values | External alignment |
|-----------|-------------|-------------------|
| `ExceptionType` | cancellation, rescheduling | iCalendar EXDATE / STATUS |
| `InvitationStatus` | needs-action, accepted, declined, tentative | iTIP PARTSTAT |
| `RsvpStatus` | needs-action, accepted, declined, tentative | iTIP PARTSTAT (invitee view) |
| `AvailabilityStatus` | free, busy, tentative, out-of-office | iCalendar FBTYPE |
| `ReminderAction` | display, email, audio | iCalendar ACTION |
| `TaskStatus` | not-started, in-progress, completed, cancelled | iCalendar STATUS |

## Projection Loss Matrices

### iCalendar (RFC 5545)

| GMEOW | iCalendar | Loss |
|-------|-----------|------|
| `EventSchedule` | `VCALENDAR` + `VEVENT` instances | Standpoint, confidence, sub-event tree |
| `ScheduleException` | `EXDATE` / `STATUS:CANCELLED` | Exception provenance, replacement event detail |
| `EventInvitation` | `ATTENDEE;PARTSTAT=...` | Reified agreement, period, confidence |
| `Availability` | `VFREEBUSY;FBTYPE=...` | Agent identity, slot provenance |
| `Reminder` | `VALARM` | Standpoint, confidence |
| `Task` | `VTODO` | Event type vocabulary, location, participation |
| `TimeZone` | `VTIMEZONE` | Full daylight/standard expansion is solver concern |

### schema.org Schedule / EventSeries

| GMEOW | schema.org | Loss |
|-------|-----------|------|
| `EventSchedule` | `Schedule` | Recurrence structure beyond `repeatFrequency` |
| `EventSchedule.scheduleOccurrence` | `Event` (`subEvent`) | Standpoint, confidence |
| `Task` | `Event` | `eventStatus` only captures cancelled/not-cancelled |
| `EventInvitation` | `RsvpAction` | Agreement structure, period, confidence |

### OWL-Time

| GMEOW | OWL-Time | Loss |
|-------|----------|------|
| `Availability.availabilitySlot` | `time:Interval` | Availability status is not temporal |
| `Task.taskDueDate` | `time:hasTime` / `time:Instant` | Task status, priority |
| `EventSchedule` | `time:Schedule` (if used) | Recurrence rule semantics |

## Build Order

Depends only on the built `events` + `temporal` modules. Phases:

1. Schedule + recurrence/exceptions
2. Invitation/RSVP + availability
3. Reminders + tasks + time zones
4. Extend ical + add jcal/schema.org Schedule projections
