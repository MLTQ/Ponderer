# Isolated event capture

Standard-library-only loopback WebSocket receiver used by the real conversation
regression. Authenticates the handshake, validates the upgrade, handles frame
boundaries and ping/pong, and captures raw generation events separately from
chat messages. It does not send operator messages or modify app settings.
Only the surrounding isolated regression controls its temporary backend.
