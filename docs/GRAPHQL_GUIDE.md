# StarForge GraphQL API

Complete GraphQL API for StarForge functionality with subscriptions and authentication.

## Features

✅ **Query all entities** - Wallets, contracts, templates, transactions, accounts
✅ **Mutations** - Create wallets, deploy contracts, submit transactions
✅ **Real-time subscriptions** - Wallet updates, transactions, contract events
✅ **GraphQL playground** - Interactive query builder
✅ **Authentication** - Bearer token based
✅ **Rate limiting** - Per-user request limits

## Quick Start

### Start Server

```bash norun
cargo run --bin starforge graphql --port 8000
```

Server runs on `http://localhost:8000`

### Access Playground

Visit: `http://localhost:8000`

## Queries

### Get All Wallets

```graphql
query {
  wallets {
    id
    publicKey
    name
    balance
    network
    funded
    createdAt
  }
}
```

### Get Wallet by ID

```graphql
query {
  wallet(id: "wallet-123") {
    id
    publicKey
    name
    balance
    network
  }
}
```

### Get Account Details

```graphql
query {
  account(publicKey: "GABC123...") {
    id
    publicKey
    balance
    sequence
    nativeBalance
    createdAt
  }
}
```

### List Templates

```graphql
query {
  templates(limit: 20, offset: 0) {
    id
    name
    version
    description
    author
    tags
    downloads
    verified
    rating
    createdAt
  }
}
```

### Get Contracts

```graphql
query {
  contracts {
    id
    address
    name
    owner
    network
    version
    language
    createdAt
  }
}
```

### List Networks

```graphql
query {
  networks {
    id
    name
    networkType
    horizonUrl
    rpcUrl
  }
}
```

## Mutations

### Create Wallet

```graphql
mutation {
  createWallet(input: { name: "My Wallet", network: "testnet" }) {
    id
    publicKey
    name
    balance
    network
  }
}
```

### Fund Wallet

```graphql
mutation {
  fundWallet(walletId: "wallet-123", amount: 100.0) {
    id
    balance
    funded
  }
}
```

### Create Contract

```graphql
mutation {
  createContract(
    input: {
      name: "Counter"
      address: "C123..."
      language: "rust"
      network: "testnet"
    }
  ) {
    id
    address
    name
    version
  }
}
```

### Deploy Contract

```graphql
mutation {
  deployContract(
    walletId: "wallet-123"
    contractId: "contract-456"
    network: "testnet"
  )
}
```

### Submit Transaction

```graphql
mutation {
  submitTransaction(
    input: {
      source: "GABC..."
      destination: "GDEF..."
      amount: 10.0
      network: "testnet"
    }
  ) {
    id
    source
    destination
    amount
    status
    createdAt
  }
}
```

### Invoke Contract

```graphql
mutation {
  invokeContract(
    contractId: "contract-123"
    method: "transfer"
    args: "{\"from\": \"...\", \"to\": \"...\"}"
  )
}
```

## Subscriptions

### Watch Wallet Updates

```graphql
subscription {
  walletUpdates(walletId: "wallet-123") {
    id
    balance
    funded
    updatedAt
  }
}
```

### Watch Transaction Updates

```graphql
subscription {
  transactionUpdates(accountId: "GABC...") {
    id
    source
    destination
    amount
    status
    confirmedAt
  }
}
```

### Watch Contract Events

```graphql
subscription {
  contractEvents(contractId: "contract-123")
}
```

### Watch Template Updates

```graphql
subscription {
  templateUpdates {
    id
    name
    version
    rating
    downloads
  }
}
```

## Schema

### Types

**Wallet**

- id: String
- publicKey: String
- name: String
- balance: Float
- network: String
- createdAt: String
- funded: Boolean

**Contract**

- id: String
- address: String
- name: String
- owner: String
- network: String
- createdAt: String
- version: String
- language: String

**Template**

- id: String
- name: String
- version: String
- description: String
- author: String
- tags: [String]
- downloads: Int
- verified: Boolean
- rating: Float
- createdAt: String

**Transaction**

- id: String
- source: String
- destination: String
- amount: Float
- fee: Float
- status: String
- createdAt: String
- confirmedAt: String (optional)
- hash: String (optional)

**Account**

- id: String
- publicKey: String
- balance: Float
- sequence: Int
- nativeBalance: Float
- createdAt: String

**Network**

- id: String
- name: String
- networkType: String
- horizonUrl: String
- rpcUrl: String

### Input Types

**CreateWalletInput**

- name: String!
- network: String!

**CreateContractInput**

- name: String!
- address: String!
- language: String!
- network: String!

**CreateTransactionInput**

- source: String!
- destination: String!
- amount: Float!
- network: String!

## Authentication

### Bearer Token

```bash norun
curl -H "Authorization: Bearer YOUR_TOKEN" \
  -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"query { wallets { id } }"}'
```

### GraphQL Header

In GraphQL playground:

1. Click "HTTP HEADERS" at bottom
2. Add: `{"Authorization": "Bearer YOUR_TOKEN"}`

## Rate Limiting

- 100 requests/minute per user
- 1000 requests/hour per user
- 10 subscriptions per user

Headers indicate limits:

- `X-RateLimit-Limit`
- `X-RateLimit-Remaining`
- `X-RateLimit-Reset`

## Performance

| Operation              | Time   |
| ---------------------- | ------ |
| Query wallets          | <50ms  |
| Query contracts        | <100ms |
| Create wallet          | <150ms |
| Submit transaction     | <500ms |
| Subscription handshake | <100ms |

## Error Handling

GraphQL errors follow standard format:

```json
{
  "errors": [
    {
      "message": "Wallet not found",
      "extensions": {
        "code": "NOT_FOUND"
      }
    }
  ]
}
```

## Client Libraries

### JavaScript

```javascript
const query = `
  query {
    wallets {
      id
      name
      balance
    }
  }
`;

const response = await fetch("http://localhost:8000/graphql", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    Authorization: "Bearer token",
  },
  body: JSON.stringify({ query }),
});

const data = await response.json();
```

### Python

```python
import requests

query = """
  query {
    wallets {
      id
      name
      balance
    }
  }
"""

response = requests.post(
  'http://localhost:8000/graphql',
  json={'query': query},
  headers={'Authorization': 'Bearer token'}
)

data = response.json()
```

### cURL

```bash norun
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer token" \
  -d '{
    "query": "query { wallets { id name balance } }"
  }'
```

## Introspection

GraphQL schema introspection available:

```graphql
query {
  __schema {
    types {
      name
      description
    }
    queryType {
      fields {
        name
        description
      }
    }
  }
}
```

## Documentation

- [Implementation Guide](./GRAPHQL_IMPLEMENTATION.md)
- [Acceptance Criteria](./GRAPHQL_ACCEPTANCE.md)
- [Performance Benchmarks](./GRAPHQL_PERFORMANCE.md)

## Support

- GitHub: https://github.com/Nanle-code/StarForge
- Issues: https://github.com/Nanle-code/StarForge/issues
