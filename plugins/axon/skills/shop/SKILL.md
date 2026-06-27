---
name: shop
description: Use when the user wants Axon-backed product research, product comparisons, review synthesis, budget-aware recommendations, or cart preparation without checkout.
---

# Axon Shop

Use this to research products and recommend a purchase option. Never submit an
order, complete checkout, enter payment credentials, accept subscriptions, or
make purchases.

## Onboarding Interview

Infer the product, budget, preferences, sites, and desired stopping point from context. If the product is clear, proceed immediately.

Ask at most 1-3 concise questions only if blocked, such as the product, hard budget/preferences, or whether cart actions are allowed.

## Axon Collection Plan

Use `axon search`, `axon research`, and `axon scrape` to compare reviews,
product pages, specifications, pricing, Reddit/forums, and trusted review sites.
Use browser automation only as a separate host capability for site navigation or
cart preparation when authorized; do not represent that as Axon-native.

## Process

1. Research product options across multiple sources.
2. Compare price, specs, reviews, seller quality, shipping, and fit to preferences.
3. Pick the best option and explain why.
4. If the user asked for cart actions, prepare a cart-ready summary or add an
   item to cart with a separate authorized browser tool, then stop for the user
   to complete checkout manually.

## Final Deliverable

```markdown
# Shopping Research: [Product]

## Recommendation
[Best option and why]

## Products Compared
[Product/model, seller, price observed at, availability, shipping/returns, warranty, key specs, pros/cons, source quality, deal risks]

## Review Signals
[Patterns from reviews and external sources]

## Cart Status
[Only if requested: item added, price, seller, confirmation]

## Sources
[URLs used]

## Rerun Inputs
workflow: shop
query: [product]
budget: [budget]
sites: [preferred sites]
```

## Quality Bar

- Be specific with model numbers, prices, and sellers.
- Never purchase, submit checkout, enter payment credentials, accept
  subscriptions, or make financial commitments.
- Note affiliate, sponsored, or unreliable sources when visible.
