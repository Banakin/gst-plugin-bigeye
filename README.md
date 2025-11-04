# gst-plugin-bigeye
A libuvc based plugin for streaming camera data from the Bigscreen Beyond 2e. Unaffiliated with Bigscreen. Inspired by some shortcomings from the [go-bsb-cams](https://github.com/LilliaElaine/go-bsb-cams) project.

## Errors
Err:
```
Could not open UVC device: Access
```

Fix:
```shell
chmod 0666 /dev/bus/usb/{BUS}/{DEVICE}
```
Find BUS and DEVICE using `lsusb`.