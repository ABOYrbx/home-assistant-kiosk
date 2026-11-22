# Departures Entity Contract

The kiosk does not talk to a transit provider directly. It reads a list of departures from one Home Assistant sensor. This keeps the application provider-neutral: every user can use the transit integration that fits their region, and no provider URL, stop ID, or personal location is compiled into the project.

## Required shape

Configure one sensor entity in `config.example.toml`:

```toml
[widgets.departures]
enabled = true
entity = "sensor.departures"
title = "Departures"
max_items = 6
refresh_seconds = 60
```

The sensor must expose a `departures` attribute containing a list. Every list entry is an object:

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `line` | string | yes | Line or route label, for example `"42"` or `"S1"`. |
| `destination` | string | yes | Displayed destination, for example `"Central Station"`. |
| `departure` | string | yes | Planned or expected departure time. RFC 3339 with an offset is preferred; `YYYY-MM-DD HH:MM:SS` is accepted and interpreted as local device time. |
| `delay_minutes` | integer | no | Expected delay in minutes. Default `0`. Negative values are accepted and displayed as “early”. |
| `platform` | string | no | Platform, stop, or bay label. |
| `cancelled` | boolean | no | `true` renders the entry as cancelled. Default `false`. |
| `remarks` | string | no | Short free-text note, for example a replacement service. |

Unknown fields are ignored. The kiosk does not depend on the sensor’s state, only on its attributes.

### Example attribute

```json
{
  "departures": [
    {
      "line": "42",
      "destination": "Central Station",
      "departure": "2026-11-22T21:55:00+01:00",
      "delay_minutes": 2,
      "platform": "2"
    },
    {
      "line": "S1",
      "destination": "Airport",
      "departure": "2026-11-22 22:03:00",
      "cancelled": true,
      "remarks": "Replacement bus"
    }
  ]
}
```

## Behaviour

- Entries are sorted by `departure` ascending.
- Entries whose departure is more than 60 seconds in the past are not shown.
- At most `max_items` entries are rendered. The default is 6; the allowed range is 1–20.
- A cancelled entry stays in the list, is visibly marked, and does not count as a valid upcoming departure for sorting purposes beyond its timestamp.
- If the attribute is missing, empty, or every entry is invalid, the widget shows “No departures” (empty list) or “Data unavailable” (missing/invalid attribute) — not a blank area.
- An invalid entry is skipped and logged at debug level. It never crashes the widget or hides valid entries.
- The list is re-read whenever the sensor’s state or attributes change. `refresh_seconds` is a safety refresh, not the only update path.
- Keep the attribute small: 20 entries is enough for a kiosk. Large lists increase WebSocket traffic and memory use.

## Creating the sensor without a provider-specific integration in the kiosk

If your transit integration exposes a different attribute shape, create a template sensor in Home Assistant that maps it to this contract. The following example is intentionally provider-neutral; replace `sensor.provider_departures` and the field names with the ones your integration actually provides.

```yaml
template:
  - sensor:
      - name: "Kiosk departures"
        unique_id: kiosk_departures
        state: "{{ now().isoformat() }}"
        attributes:
          departures: >-
            {% set source = state_attr('sensor.provider_departures', 'departures') or [] %}
            {% set ns = namespace(items=[]) %}
            {% for item in source %}
              {% set ns.items = ns.items + [{
                'line': item.line | default('?'),
                'destination': item.destination | default('Unknown'),
                'departure': item.departure | default(item.when),
                'delay_minutes': item.delay_minutes | default(0) | int,
                'platform': item.platform | default(none),
                'cancelled': item.cancelled | default(false) | bool,
                'remarks': item.remarks | default(none)
              }] %}
            {% endfor %}
            {{ ns.items }}
```

A template sensor updates when its source entity changes, so the kiosk receives the list through the normal `subscribe_entities` stream. No polling URL is added to the kiosk.

## Why this contract instead of a provider adapter

- **Portability:** the same binary works in every region and with every transit integration.
- **No secrets or personal data:** no stop ID, home address, or provider account is stored in the repository.
- **Testability:** the parser has one input shape and can be tested with fixtures.
- **Upgrade path:** if a generic HTTP adapter is needed later, it can be added as a second source without changing the widget or the contract.

## Non-goals

- Real-time vehicle positions.
- Trip planning or routing.
- Ticket purchase.
- Provider-specific authentication inside the kiosk.
