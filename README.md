# sigma-cart

Shopping cart service for Sigma Tactical Group. Stores carts locally, pulls catalog SKUs and identity users from upstream services, with a server-rendered web UI and JSON API.

Repository: https://github.com/sigmatactical-org/cart

Shared site chrome comes from [sigma-theme](https://github.com/sigmatactical-org/sigma-theme).

## Features

- **Catalog integration** — validate and enrich line items from [sigma-catalog](https://github.com/sigmatactical-org/catalog)
- **Identity integration** — assign carts to users via Keycloak Admin API (same realm as [sigma-identity](https://github.com/sigmatactical-org/identity))
- **Web UI** — browse carts, edit details, add and remove line items
- **JSON API** — programmatic CRUD for integration behind sigma-identity

## Configuration

| Variable | Purpose |
|----------|---------|
| `PORT` | Listen port (default `8080`) |
| `CART_DATA_PATH` | JSON database path (default `data/carts.json`) |
| `CART_CATALOG_BASE_URL` | Catalog service base URL (e.g. `http://127.0.0.1:8081/`) |
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

## API

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

```bash
./scripts/prepare-local.sh
cargo run -p sigma-cart
```

Open http://localhost:8080

Example local integration:

```bash
export CART_CATALOG_BASE_URL=http://127.0.0.1:8081/
export CART_IDENTITY_ISSUER_URL=http://127.0.0.1:8101/realms/multcorp
export CART_IDENTITY_CLIENT_ID=identity
export CART_IDENTITY_CLIENT_SECRET=8d476311-2577-4104-b9e4-7dc2cc381be8
cargo run -p sigma-cart
```

## Docker

Release is in **`.github/workflows/release.yml`** when configured. Locally:

```bash
./scripts/docker-build.sh
docker build -f Dockerfile build/image
```

Mount a volume at `/app/data` (or set `CART_DATA_PATH`) so cart data persists across restarts.

## License

MIT OR Apache-2.0
