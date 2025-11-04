#!/bin/bash
./scripts/env.sh && ./scripts/build.sh && gst-inspect-1.0 target/debug/libgstbigeye.so