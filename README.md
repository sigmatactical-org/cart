# sigma-cart

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.97.0-blue.svg)](https://www.rust-lang.org)

Public shopping cart service for Sigma Tactical Group. It owns the customer-facing cart UI that storefronts (e.g. [sigma-store](https://github.com/sigmatactical-org/store)) add items to, plus an internal admin UI and JSON API. Carts are stored locally; catalog SKUs come from [sigma-catalog](https://github.com/sigmatactical-org/catalog), prices from the store, and users from identity.

## Public vs internal

- **Public** (`cart.sigmatactical.store`): `GET /` (the shopper's cart), `POST /add`, the line quantity/remove actions, and `POST /reserve` (pay the 50% deposit to reserve). No admin data is rendered on these pages.
- **Internal / admin only**: `GET /admin` and the `/admin/carts/*` CRUD pages, plus the JSON API. These are not linked from the public pages and are intended to be reached only through the [sigma-identity](https://github.com/sigmatactical-org/identity) authenticated proxy in production.

Repository: https://github.com/sigmatactical-org/cart

Shared site chrome comes from [sigma-theme](https://github.com/sigmatactical-org/sigma-theme).

## Features

- **Public cart UI** — line items with quantity steppers, remove, live totals, and a 50%-deposit reserve flow gated by identity sign-in
- **Add to cart** — `POST /add` accepts a catalog `sku_id` from any storefront; a guest cart is created on first add and tracked by a shared `sigma_cart` cookie
- **Catalog integration** — validate and enrich line items from [sigma-catalog](https://github.com/sigmatactical-org/catalog)
- **Pricing** — resolves authoritative unit prices from the store's `/items` feed (prices live on store listings, not the catalog)
- **Orders** — paying the deposit creates an order in [sigma-orders](../orders) and marks the cart submitted
- **Identity integration** — assign carts to users via Keycloak Admin API (same realm as [sigma-identity](https://github.com/sigmatactical-org/identity))
- **Admin web UI + JSON API** — browse/edit carts behind sigma-identity

## Configuration

| Variable | Purpose |
|----------|---------|
| `PORT` | Listen port (default `8080`) |
| `DATABASE_URL` | PostgreSQL connection URL (default `postgres://sigma:sigma@127.0.0.1:5432/sigma`) |
| `CART_CATALOG_BASE_URL` | Catalog service base URL (e.g. `http://127.0.0.1:8081/`) |
| `CART_STORE_BASE_URL` | Store service base URL for authoritative listing prices (e.g. `http://127.0.0.1:8082/`) |
| `CART_PUBLIC_BASE_URL` | Canonical public URL of this cart, for sign-in return links (default `http://127.0.0.1:8084/`) |
| `CART_IDENTITY_PUBLIC_URL` | Public identity BFF base URL for the reserve sign-in gate (default `http://127.0.0.1:3000/`) |
| `CART_CONTACT_PUBLIC_URL` | Public contact service URL for the navbar link (default `http://127.0.0.1:8083/`) |
| `CART_STORE_PUBLIC_URL` | Public store URL for product and continue-shopping links (default `http://127.0.0.1:8082/`) |
| `CART_ORDERS_BASE_URL` | Orders service base URL for checkout commit (e.g. `http://127.0.0.1:8085/`) |
| `CART_COOKIE_DOMAIN` | Cookie `Domain` so the `sigma_cart` cookie is shared with the storefront across sibling subdomains; leave blank in local dev |
| `CART_IDENTITY_ISSUER_URL` | OIDC issuer / realm URL (e.g. `http://127.0.0.1:8101/realms/multcorp`) |
| `CART_IDENTITY_CLIENT_ID` | Service-account client id for Admin API |
| `CART_IDENTITY_CLIENT_SECRET` | Service-account client secret |

Identity lookup requires a Keycloak client with **service accounts enabled** and the **view-users** role on **realm-management**.

## Data model

Each cart has:

- optional `user_id` — identity user id
- `status` — `open`, `submitted`, or `cancelled`
- optional `note`
- `lines` — `[{ "sku_id", "quantity" }, …]` (only editable when status is `open`)

Reserving a cart creates an **order** in sigma-order (customer, line items with unit/line prices, and the 50% deposit) and flips the cart to `submitted`.

## Public routes

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | The shopper's cart (guest cart via the `sigma_cart` cookie) |
| `POST` | `/add` | Add a catalog `sku_id` (form field); creates a guest cart on first add |
| `POST` | `/lines/{line_id}/increment` | Increase quantity |
| `POST` | `/lines/{line_id}/decrement` | Decrease quantity (removes at 0) |
| `POST` | `/lines/{line_id}/remove` | Remove the line |
| `POST` | `/reserve` | Reserve by paying the 50% deposit (requires identity sign-in) |

## Admin + JSON API

The admin web UI is mounted under `/admin`. The JSON API (reached through sigma-identity) is unchanged:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/users` | List identity users |
| `GET` | `/carts` | List carts (enriched with catalog and identity) |
| `GET` | `/carts/{id}` | Get one cart |
| `POST` | `/carts` | Create cart (JSON) |
| `PUT` | `/carts/{id}` | Update cart |
| `DELETE` | `/carts/{id}` | Delete cart |
| `POST` | `/carts/{id}/lines` | Add line item |
| `PUT` | `/carts/{id}/lines/{line_id}` | Update line quantity |
| `DELETE` | `/carts/{id}/lines/{line_id}` | Remove line |

Example create cart:

```json
{
  "user_id": "<identity-user-id>",
  "note": "Quote request"
}
```

Example add line:

```json
{
  "sku_id": "<catalog-sku-id>",
  "quantity": 2
}
```

### Behind sigma-identity

Point identity at this service, for example:

```bash
IDENTITY_PROXY_TARGET=http://127.0.0.1:8080/
```

Browser clients call `/api/carts` on the identity host (with session + CSRF); identity forwards the request with a Bearer token attached.

## Development

Standalone clone:

```bash
./scripts/prepare-local.sh
cargo run -p sigma-cart
```

Under the sigma workspace (`sigma/it/cart`):

```bash
cd sigma/it/cart && ./scripts/prepare-local.sh && cargo run -p sigma-cart
# or prepare all commerce services:
(cd sigma/it && ./scripts/prepare-commerce-local.sh)
(cd sigma/it && cargo run -p sigma-cart)
```

Open http://localhost:8080

Example local integration:

```bash
export CART_CATALOG_BASE_URL=http://127.0.0.1:8081/
export CART_STORE_BASE_URL=http://127.0.0.1:8082/
export CART_STORE_PUBLIC_URL=http://127.0.0.1:8082/
export CART_PUBLIC_BASE_URL=http://127.0.0.1:8084/
export CART_IDENTITY_PUBLIC_URL=http://127.0.0.1:3000/
export CART_IDENTITY_ISSUER_URL=http://127.0.0.1:8101/realms/multcorp
export CART_IDENTITY_CLIENT_ID=identity
export CART_IDENTITY_CLIENT_SECRET=8d476311-2577-4104-b9e4-7dc2cc381be8
PORT=8084 cargo run -p sigma-cart
```

## Docker

Release is in **`.github/workflows/release.yml`** when configured. Locally:

```bash
./scripts/docker-build.sh
docker build -f Dockerfile build/image
```

Data is stored in the shared PostgreSQL `cart` schema (`cart.carts` and `cart.cart_lines` relational tables). Postgres runs in the [platform](https://github.com/sigmatactical-org/platform) kind stack — port-forward for local `cargo run`:

```bash
cd platform && ./scripts/postgres-dev.sh port-forward-bg && ./scripts/postgres-dev.sh migrate
```

## Brand & artwork

© Sigma Tactical Group. **All rights reserved.**

The Sigma Tactical Group name, logos, marks, artwork, and visual identity are **proprietary**. They are not covered by this repository's source-code license. See [BRANDING.md](BRANDING.md).

## License

MIT OR Apache-2.0 for **source code** only. Branding remains proprietary.
