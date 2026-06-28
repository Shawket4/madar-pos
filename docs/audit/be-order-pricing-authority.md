I have everything I need. Let me confirm the `Order` struct doesn't expose `price_flagged`/`price_expected_total` (it ends well before those would appear — the RETURNING clause at line 1193-1204 doesn't select them either, confirming they're write-only). Bundle pricing on the server: the bundle's base `price` is a stored column on the `bundles` table (set at create/update time with margin-floor validation), and at order time it's read verbatim from `bundles.price` (line 832 `bundle.2`); the component surcharge adds only catalog addon/optional prices.

I have a complete, definitive picture.

---

# VERDICT: Hybrid, but effectively **client-authoritative for recorded money** — the server is a passive recorder + advisory deviation flagger, never an enforcer.

The POS sends the priced breakdown and the backend **records it verbatim**. The server computes its own "expected" totals purely to set an advisory `price_flagged` boolean and stash `price_expected_total` — and **nothing in production ever reads those columns**. Orders are **never rejected on a price/total/tax/discount/bundle mismatch**. The only money-related hard reject is split-payments not summing to `total_amount`.

## (1) Field-by-field: client SENDS vs server DERIVES

Create path: `create_order` in `/Users/shawket/Desktop/MadarRust/src/orders/handlers.rs` (struct ~L270–315, logic ~L611–1058, INSERT ~L1180–1230).

| Monetary field | Client may send? | What the server does with it | Recorded value |
|---|---|---|---|
| `OrderItemInput.unit_price` (per line) | Yes (`Option<i32>`, L286) | If present, **recorded verbatim** as the line's charged unit price; if absent, falls back to server "expected" (catalog + branch override). L967–968 | client value (or server expected) |
| `subtotal` | Yes (`Option<i32>`, L310) | `body.subtotal.unwrap_or(<sum of charged lines>)` — **client value taken verbatim**; only computed if omitted. L1034 | client value (or Σ charged lines) |
| `discount_amount` | Yes (`Option<i32>`, L311) | `body.discount_amount.unwrap_or_else(|| calc_discount(subtotal)).clamp(0, subtotal)` — **client value taken verbatim**, only clamped to `[0, subtotal]`. L1035 | client value clamped (or server calc) |
| `tax_amount` | Yes (`Option<i32>`, L312) | `body.tax_amount.unwrap_or_else(|| (taxable * rate).round())` — **client value verbatim**; only computed if omitted. L1037 | client value (or server calc) |
| `total_amount` | Yes (`Option<i32>`, L313) | `body.total_amount.unwrap_or(taxable + tax_amount)` — **client value verbatim**; only computed if omitted. L1038 | client value (or server calc) |
| `change_given` | Yes (`Option<i32>`, L314) | client value, else `amount_tendered - total_amount` clamped ≥0. L1039 | client value (or derived) |
| `discount_type` / `discount_value` | Yes (L296–297) | Used as-is unless `discount_id` is sent | client value (or resolved from DB) |
| `discount_id` | Yes (L298) | If present, **server overrides** `type`/`value` from the `discounts` table (must exist + be active, else reject). L530–542 | server-resolved |
| `branch_id` | Sent | **Server overrides** with the shift's branch (`shift_branch_id`) for the order row. L1207 | server-authoritative |
| `bundle base price` | No (not in input) | **Server-derived**: read verbatim from `bundles.price` at order time (L832, `bundle.2`). Recorded as `bundle_unit_price`. | server-authoritative |
| `bundle component_surcharge` | No | **Server-derived** from catalog addon/optional prices of components (L774). | server-authoritative |
| line cost / `line_cost` / `unit_cost` | No | **Server-derived** point-in-time ingredient costs (L1066–1089). | server-authoritative |
| `price_flagged`, `price_expected_total` | No | **Server-derived** advisory only (see §4). | server-authoritative, write-only |

## (2) Client totals present vs absent

- **Present (current POS):** recorded **verbatim** (subject only to `discount_amount.clamp(0, subtotal)`). The server independently recomputes an **expected** breakdown over catalog+override prices (`expected_subtotal/discount/tax/total`, L1024–1029), compares, and sets `price_flagged = any line deviated OR subtotal != expected_subtotal OR total_amount != expected_total` (L1056–1058). It stores `price_expected_total` for reconciliation. **It never rejects** (confirmed: no `return Err` on any total/tax/subtotal/price/discount mismatch; grep for mismatch-rejects returns nothing in the create path).
- **Absent (legacy / pre-update builds / tests):** server computes (§3 formula) over the charged-line subtotal it accumulated.

## (3) The server's formula (what Rust must match)

All piastres (integer). `calc_discount` is the single source of truth in `/Users/shawket/Desktop/MadarRust/src/discounts/handlers.rs` L270–277:

```
discount = match dtype {
  "percentage" => round_half_away_from_zero(subtotal * value / 100.0),
  "fixed"      => min(value, subtotal),
  _            => 0,
}.clamp(0, subtotal)

taxable = subtotal - discount
tax     = round_half_away_from_zero(taxable * tax_rate)   // tax_rate from organizations.tax_rate (f64, default 0.14)
total   = taxable + tax
```
- Line subtotal: `(unit_price + addon_per_unit + optional_per_unit) * quantity + component_surcharge` (L981–983).
- Discount/tax apply to the **whole-order subtotal** (post-discount taxable base), not per line.
- Rounding is `.round()` (Rust round-half-away-from-zero) — explicitly noted as matching the POS preview "to the piastre."

## (4) Where the deviation is recorded / consumed

- Written to `orders.price_flagged`, `orders.price_expected_total`, and `order_items.price_flagged` (migration `/Users/shawket/Desktop/MadarRust/migrations/20260614120000_branch_menu_overrides.sql` L31–36).
- **Nothing consumes them in production.** Grep across all non-test `.rs`: the only reads of `price_flagged`/`price_expected_total` are in `orders/tests.rs`. No report, no list filter, no aggregate (the `OrderSummary` revenue/discount sums use `total_amount`/`discount_amount`, not the flag), and the `Order` response struct (L62+) and the INSERT `RETURNING` clause (L1193–1204) **do not even expose these columns** to the API. They are write-only audit residue.

## (5) Bundle pricing on the server

- Bundle base price is **stored** on `bundles.price`, validated at create/update time only (margin floor `price ≥ 1.20 * Σcosts`, perceivability `price ≤ 0.97 * Σlist_prices` — `bundles/handlers.rs` L370–384). Not recomputed at order time.
- At order time the bundle line uses `bundles.price` verbatim as both expected `unit_price` and `bundle_unit_price` (L832). Component swaps must match catalog quantity exactly (else reject, L755–760). The only additive money is `component_surcharge` = catalog addon+optional prices of components (L774). For bundle lines, item-level addon/optional per-unit is forced to 0 (L970–979) — surcharge is the sole add-on. The client's `unit_price` overlay (L968) can still override the bundle's recorded charged price; deviation just flags.

## Implication for the Rust pricing engine

- **The backend will not protect you.** Whatever the POS computes is what gets stored and what shows on reports/revenue. So the Rust pricing core in the POS is the **de facto source of truth** for every monetary field — totals, tax, discounts, and bundle line prices must be correct client-side; there is no server recompute-and-override safety net.
- **Match the server formula exactly** to keep `price_flagged` clean (avoid false deviations): integer piastres; discount via the `calc_discount` rules (percentage rounds half-away-from-zero, fixed capped at subtotal, clamp `[0, subtotal]`); tax = `round(taxable * tax_rate)` on the post-discount whole-order base; `total = subtotal - discount + tax`. Pull `tax_rate` from the org (default 0.14) and `discount` from `discount_id` resolution when an id is used (server overrides type/value in that case).
- **Bundle base price** must come from the synced `bundles.price`; the engine adds only the per-component addon/optional surcharge, and must enforce exact catalog component quantities (server rejects mismatches).
- **Send the full breakdown** (`unit_price` per line + `subtotal`/`discount_amount`/`tax_amount`/`total_amount`) so the DB equals the printed receipt; omitting them silently hands authority back to the server's catalog computation, which may differ from a stale/offline POS.

Key files: `/Users/shawket/Desktop/MadarRust/src/orders/handlers.rs` (create path L470–1335), `/Users/shawket/Desktop/MadarRust/src/discounts/handlers.rs` (`calc_discount` L270), `/Users/shawket/Desktop/MadarRust/src/bundles/handlers.rs` (bundle price validation L355–387), `/Users/shawket/Desktop/MadarRust/migrations/20260614120000_branch_menu_overrides.sql` (deviation columns).