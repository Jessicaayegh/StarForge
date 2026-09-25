# SEP-10 web authentication

[SEP-10](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0010.md)
is how a wallet or dApp proves to an anchor that it controls a Stellar account,
and gets a session JWT back. The flow is a challenge/response:

1. The client asks the server for a **challenge** — an unsigned Stellar
   transaction that only the account being authenticated can pay a fee on
   (its sequence number is `0`, so it can never be submitted to the network).
2. The client **validates** the challenge against the server it asked, then
   **signs** it with the account's key.
3. The server verifies the client signature *and its own*, and returns a
   **session token**.

`starforge sep10 auth` runs that whole flow from the CLI so you can debug an
anchor integration without writing a throwaway script.

## Authenticate

```bash norun
starforge sep10 auth --domain testanchor.stellar.org --wallet alice
```

`--domain` is the home domain that publishes the server's `stellar.toml`;
`--wallet` names a saved wallet (see `starforge wallet list`) whose account is
the one being authenticated. The session token is printed on its own line, so
it can be piped straight into the next request:

```bash norun
TOKEN=$(starforge sep10 auth --domain testanchor.stellar.org --wallet alice)
curl -H "Authorization: Bearer $TOKEN" https://testanchor.stellar.org/deposit
```

The account never has to be funded, and no transaction is submitted.

## Watch each step

`--verbose` prints the discovery data, every step of the flow, and each
validation rule the challenge satisfied:

```bash norun
starforge sep10 auth --domain testanchor.stellar.org --wallet alice --verbose
```

The output names the server's `SIGNING_KEY` and `WEB_AUTH_ENDPOINT`, the
network passphrase the challenge was signed for, the home domain, the data
name, the nonce, the time bounds, and how many operations the challenge
carried. When an anchor rejects a request, that ordering makes it obvious which
of the two sides disagreed.

## Choosing what the server returns

Two SEP-10 extensions let the client constrain the challenge:

```bash norun
# Ask the server to echo a memo in the challenge (G... accounts only).
starforge sep10 auth --domain testanchor.stellar.org --wallet alice --memo 12345

# Ask the server to pin a client domain in the challenge.
starforge sep10 auth --domain wallet.example.com --wallet alice \
  --client-domain wallet.example.com
```

`--memo` is validated on the way back: a challenge whose memo differs from the
one requested is rejected before it is signed.

## Machine-readable output

`--json` (or the global `--json`) emits the framework's standard envelope
instead of the bare token:

```bash norun
starforge sep10 auth --domain testanchor.stellar.org --wallet alice --json
```

```json norun
{
  "version": 1,
  "ok": true,
  "data": {
    "account": "G...",
    "home_domain": "testanchor.stellar.org",
    "web_auth_endpoint": "https://testanchor.stellar.org/auth",
    "network_passphrase": "Test SDF Network ; September 2015",
    "jwt": "eyJ...",
    "steps": [
      { "step": "discover", "detail": "G... via https://testanchor.stellar.org/auth" },
      { "step": "fetch", "detail": "challenge for G... (568 bytes of XDR)" },
      { "step": "validate", "detail": "1 operation(s), nonce SMf... , expires 1750000000" },
      { "step": "sign", "detail": "signed with G..." },
      { "step": "submit", "detail": "session token received (892 characters)" }
    ],
    "checks": [["home domain", "testanchor.stellar.org"], ["nonce", "SMf..."]]
  }
}
```

The session token is the field named `jwt`; the `checks` list is the same set
of validated fields `--verbose` prints.

## What gets validated

A challenge is rejected before any key is used to sign it. Each rule has its
own error, so a failure names the one thing that is wrong rather than
"invalid challenge":

| Rule | Rejected when |
|---|---|
| Base64 | The `transaction` field is not valid base64. |
| XDR | The decoded bytes are not a transaction envelope. |
| Envelope version | The envelope is not a v1 (`ENVELOPE_TYPE_TX`) transaction. |
| Server account | The transaction's source account is not `SIGNING_KEY`. |
| Sequence number | The challenge sequence is not `0`. |
| Time bounds | Time bounds are missing, already expired, or inverted. |
| First operation | It is not a `ManageData` operation named `<home domain> auth`. |
| Operation account | The first operation is not signed for by the account authenticating. |
| Nonce | The nonce is not 64 base64 characters decoding to 48 raw bytes. |
| Extra operations | A later operation is not `ManageData`, or names something other than the server's own `web_auth_domain` / the requested `client_domain`. |
| `web_auth_domain` value | It names a domain the client never contacted. |
| `client_domain` value | It differs from the `client_domain` the client requested. |
| Memo | The memo is not `MEMO_ID`, or differs from the one requested. |
| Server signature | No signature on the challenge verifies against `SIGNING_KEY` and the selected network passphrase. |

Signing is refused until every rule above passes, so a server that answers with
somebody else's challenge cannot get your key to sign it.

## Discovery

`--domain` is resolved through SEP-1: StarForge fetches
`https://<domain>/.well-known/stellar.toml`, reads `SIGNING_KEY`,
`WEB_AUTH_ENDPOINT`, and `NETWORK_PASSPHRASE`, and stops with a clear error if
the anchor does not publish them. Only `https` endpoints are accepted, except
for a **loopback** host (`localhost`, `127.0.0.1`, `[::1]`), so a local
reference server can be exercised with `http`.

Both `SIGNING_KEY` and `WEB_AUTH_ENDPOINT` are also accepted in lower case,
which older anchors and local test fixtures in the wild still use. The network
passphrase defaults to the public network when the document omits it, and a
challenge that arrives with a *different* passphrase than discovery advertised
is rejected rather than signed.

Redirects are not followed on either the discovery or the challenge/token
request: the signed challenge and the session token are bearer credentials, and
SEP-10 endpoints are required to answer directly.

## Reference server in tests

The validator is a pure function over decoded XDR, so the protocol rules are
tested against challenges built in-process, with no server in the loop. The two
network calls are covered by a **local reference server**: `utils::sep10`'s
tests stand up an HTTP server that serves a `stellar.toml` at
`/.well-known/stellar.toml`, hands out a challenge signed by the server key,
and returns a token for the signed challenge it is posted — then assert the
flow's steps, the validated checks, and the token. That the client's signature
is actually on the submitted envelope and verifies under the client key is
asserted separately, by decoding the signed challenge and checking the
signature against the transaction payload.

Malformed challenges — wrong source account, non-zero sequence, missing or
inverted time bounds, a non-`ManageData` operation, a wrong operation source, a
short or malformed nonce, a foreign `web_auth_domain`, an unexpected memo, a
non-v1 envelope, and a challenge signed by the wrong key — are each asserted to
fail with their specific error.

## Where this sits

`starforge sep10` covers the client half of SEP-10. It is the groundwork for
testing SEP-24 and SEP-31 deposit/withdrawal flows, which build their
authentication on the same challenge/response exchange.
