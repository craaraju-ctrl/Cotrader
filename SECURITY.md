# RAT Agent — Security Considerations

## API Key Management

- Never commit API keys to git
- Use environment variables or secure vaults
- Rotate keys regularly
- Use read-only keys where possible

## Network Security

- All API calls use HTTPS
- WebSocket connections use WSS
- No sensitive data in logs
- Rate limiting on all endpoints

## Data Protection

- Memory stored locally (SQLite)
- No cloud storage of trading data
- Encrypted at rest (if configured)
- Secure deletion of old data

## Access Control

- Paper mode default (no real money)
- Live mode requires explicit confirmation
- Position limits enforced at code level
- Daily loss limits hard-coded

## Audit Trail

- All trades logged with timestamps
- Decision reasoning recorded
- Memory entries tagged with agent namespace
- SQLite-backed persistent storage
