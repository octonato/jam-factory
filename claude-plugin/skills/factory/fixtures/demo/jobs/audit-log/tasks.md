# Tasks: audit-log

### T1: Add the log writer
- source: plan task 1
- status: done
- commit: .@d4e5f6a

Add the log writer.

### T2: Log admin actions
- source: plan task 2
- status: done
- commit: .@e5f6a7b

Call the log writer from each admin action.

### T3: Flush on shutdown
- source: round 1 F1
- status: done
- commit: .@f6a7b8c

The writer drops buffered entries when the service stops.

### T4: Add the actor id
- source: round 1 F2
- status: open
- commit: none

The log entry misses the id of the admin who acted.
