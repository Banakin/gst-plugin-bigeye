#!/bin/bash
# Run the UVC webcam source plugin with a simple test pipeline
# This will capture from the webcam and display it on screen

GST_PLUGIN_PATH=`pwd`/target/debug gst-launch-1.0 bigeyesrc ! jpegdec ! videoconvert ! autovideosink
