"""Entry point: ``python -m cq_worker``."""

from __future__ import annotations

import json
import logging
import os
import signal
import sys
import traceback

from .protocol import Channel, redirect_std_streams


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "--probe":
        from .environment import runtime_environment

        env = runtime_environment()
        sys.stdout.write(json.dumps({"ok": True, **env}) + "\n")
        sys.stdout.flush()
        return 0

    signal.signal(signal.SIGINT, signal.default_int_handler)
    channel = Channel.from_std_streams()
    redirect_std_streams(channel)

    try:
        from .server import ChannelLogHandler, Server
    except Exception as error:
        channel.send(
            {
                "type": "fatal",
                "message": f"Cannot load MusicScript engine: {type(error).__name__}: {error}",
                "detail": traceback.format_exc(),
                "executable": sys.executable,
            }
        )
        return 1

    logging.basicConfig(level=logging.INFO, handlers=[ChannelLogHandler(channel)])
    Server(channel, session=os.environ.get("CQ_EDITOR_SESSION")).serve_forever()
    return 0


if __name__ == "__main__":
    sys.exit(main())
