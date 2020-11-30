#!/bin/bash
exec /fishnet --no-conf \
    --key "$KEY"\
    --cores "$CORES"\
    --endpoint "$ENDPOINT"\
    --user-backlog "$USER_BACKLOG"\
    --system-backlog "$SYSTEM_BACKLOG"\
    run
