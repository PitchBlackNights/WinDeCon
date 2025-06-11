# WinDeCon

The name WILL change, I just don't know what to change it to yet.

Development has stalled a bit due to:

1. Personal events
2. rusb (specifically libusb which rusb wraps) keeps throwing an IO error when I try to read/write data to a device. The only thing I can think of that might be causing this is Thread Safety stuff. Maybe libusb is throwing errors because it can't guarantee the thread can safely access the device?
