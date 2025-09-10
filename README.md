```
**WARNING:** This is an old version of the plugin. The latest implementation is available in a [Merge Request to gst-plugins-rs](https://gitlab.freedesktop.org/gstreamer/gst-plugins-rs/-/merge_requests/2186).
```

<div align="center">
<img style="border-radius: 5px;" width="430" src="https://www.vicomtech.org/dist/img/logo.svg"> <br>
</div>

# GStreamer plugin for DASH CMAF Sink

This work is supported by 6G-XR project (https://6g-xr.eu/).

# :busts_in_silhouette: Leading company and participants

- **Project type:** European Research Project focused on 6G technologies and XR innovations. [More info](https://6g-xr.eu/about-6g-xr/)

- **Consortium:** European consortium focusing on the development of next-generation XR services and infrastructures. [More info](https://6g-xr.eu/consortium/)

# :dart: Objective 
The project aims to develop a multisite Research Infrastructure (RI) to validate various 6G use cases, focusing on innovative XR applications, edge computing, and beyond 5G technologies. It also targets advancements in holographics, digital twins, and immersive XR/VR applications.

# 📚 Documentation
The DASH CMAF Sink provides an alternative to the DASH implementation already available in GStreamer. The objective is to have a DASH streaming compliant with DASH IF recommendations.

# :rocket: Deployment
Install dependencies:
```bash
cargo install cargo-c
```

Build DASH CMAF plugin:
```bash
cargo build
```

Usage / Example pipeline:
```bash
gst-launch-1.0 --gst-plugin-path=target/debug/ videotestsrc is-live=true do-timestamp=true ! video/x-raw,width=1920,height=1080,framerate=60/1  ! videoconvert ! timeoverlay ! queue ! x264enc tune=zerolatency key-int-max=5 ! video/x-h264,profile=main ! dashcmafsink target-duration=2 name=dash audiotestsrc is-live=true do-timestamp=true ! audioconvert ! avenc_aac ! aacparse ! dash.
```

# :computer: Technologies used in the project
Technology stack used in the project.
- [x] GStreamer RUST

# :thumbsup: Contributions
People involved in the project:

- 👨‍💻 Roberto Viola (<rviola@vicomtech.org>)
