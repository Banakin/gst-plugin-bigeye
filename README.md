# gst-plugin-bigeye
A libuvc based plugin for streaming camera data from the Bigscreen Beyond 2e. Unaffiliated with Bigscreen. Inspired by some shortcomings from the [go-bsb-cams](https://github.com/LilliaElaine/go-bsb-cams) project.

## Installation (Fedora)
### Prerequisites
Enable copr repository:
```shell
sudo dnf copr enable rayfoxyote/nobara-42-bsb
```
Install required Udevu Rules:
```shell
sudo dnf install bigscreen-udev-rules
```
And then reboot your system.

### Install
```shell
sudo dnf install gst-plugin-bigeye
```
*Note: RPM spec files are kept [here](https://github.com/Banakin/Nobara-BSB-RPM-Sources).*

## Usage
```shell
gst-launch-1.0 bigeyesrc ! jpegdec ! videoconvert ! autovideosink
```

### Use with Baballonia
Simply use this string as your source:
```
bigeyesrc ! jpegdec ! videoconvert ! appsink
```

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
