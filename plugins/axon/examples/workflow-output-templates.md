# Workflow Output Templates

These are compact examples for Axon workflow deliverables. Adapt fields to the
user's requested format.

## QA Finding

```markdown
[M-1] /signup | Email validation accepts invalid domain
Steps: open /signup, enter user@example, submit form
Expected: inline validation error
Actual: form advances to next step
Evidence: screenshot=.axon/signup-invalid-email.png, viewport=desktop 1440x900, console=none, network=200s only, auth=anonymous
```

## Product Walkthrough Step

```markdown
1. Pricing page
   URL: https://example.com/pricing
   Action: Open page and switch monthly/annual toggle
   Observed: Annual toggle changes prices and discount copy
   Evidence: screenshot=.axon/pricing-annual.png, viewport=desktop, console=none
```

## Dashboard Metric

```json
{
  "name": "Monthly active users",
  "value": 12840,
  "unit": "users",
  "period": "2026-06",
  "sourceUrl": "https://analytics.example.com/dashboard",
  "extractedAt": "2026-06-27T00:00:00Z",
  "confidence": "observed",
  "caveats": "rounded dashboard KPI"
}
```

## Lead Row

```json
{
  "name": "Example Person",
  "title": "VP Engineering",
  "company": "ExampleCo",
  "sourceUrl": "https://example.com/team",
  "profileUrl": "https://example.com/team/example-person",
  "extractedAt": "2026-06-27T00:00:00Z",
  "fieldsObserved": ["name", "title", "company"],
  "confidence": "observed",
  "limitations": "No personal contact info collected"
}
```

