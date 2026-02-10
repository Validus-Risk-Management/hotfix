This is a dummy acceptor executor using QuickFIX/Go.
As HotFIX currently doesn't support sell-side use cases,
there is no way to set up end-to-end examples where HotFIX
acts as both sides of the connection.

The acceptor implementation here provides a "ping-pong"
server, which responds to new orders with an ack and
a fill.

## Acknowledgement

This product includes software developed by
quickfixengine.org (http://www.quickfixengine.org/).