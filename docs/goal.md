# Scientific Miscrope Instrument Software: Build Specification

I want to build a professional scientific microscope instrument software for lab applications.

The followings are the requirements:

* The microscope is branded as OScope (short for OpenScope).
* Hardware summary:
  - Camera: MIchrome 20 USB3.1. The microscope has a camera `MIChrome 20`(https://www.tucsen.com/michrome-20-product/), color, resolution 20MP, 5472(H) x 3648(V), pixel size: 2.4 μm x 2.4 μm, 14.5 fps @ 5472x3648, bit depth: 16/8bit
  - Stage: xyz grblHAL over serial port. The microscope has a motorized stage xyz talking grblHAL via serial port. xy can be used for scanning, z can be used for focusing or z-scan
  - Objectives: 5x, 10x, 20x, 50x Mitutoyo PLAN long working distance.
* Use Rust as the backend, web and Tauri 2 as the frontend. The web and Tauri 2 frontends share common Vue 3 code.
* The Rust backend consists of layered architecture.
* The Web and Tauri 2 frontend handles user controls, live view, workflow, deep-zoom visualization, 3D volume and depth rendering.
* Dual-Stream strategy:
  - Live view (Preview): In Rust, downsamplle and compress the 12-bit raw frame into a lightweight WebP/JPEG or H.264 and send the lightweight compressed preview frames to the UI as 30-60 fps.
  - Raw data (for analysis/stitching/saving): Keep the prestine 12-bit raw data strictly in Rust memor or flush it directly to a high-speed NVMe scratch disk.
* Imaging processing
  - Processing pipeline (Rust): a pipeline of nodes for image processing in Rust. If CUDA GPU is available, use the GPU, or use the CPU.
  - algorithms are implemented as processing nodes. Stock processing nodes are implemented in with the software, and can also be extended with Extensioin. Extension traits must be defined to support 3rd party Extensions. 3rd party Extensions can be implemented as Rust package or Python package. The Extension packages using .omext file name convention.
* Image stitching: dual-mode
  - real-time live stitching: as the stage moves, match each incoming frame's timestamp/index with the real-time xyz coordinates. Use a fast affine transformation pipeline backed GPU or CPU.
  - post-acquisition stitching: a feature/phase correlation based stitching.
  - 2D deep-zoom (DZI format)
* 3D processing
  - Depth-from-focus
  - Light field and Confocal volume rendering
* Use `teta-wot`(https://github.com/lucaswewa/teta-wot) as the HTTP interface
* Use `microscope`(https://github.com/lucaswewa/microscope) UI and theme (look and feel) as reference frontend design.
  - Vue3+Typescript+Vite
  - Use the reka-ui
  - Use pinia for state management
  - use material-symbols
* Frontend UI major features:
  - A NavRail consists of tabs: `Control and Live View`, `Scan Workflow`, `Workspace`, `Gallery`, `Settings`, `Jobs`, `Logging`, `About`, `Power`, `Instrument Status Indicator`
  - `Control and Live View` consists of:
    + Camera control: exposure time, gain, pixel format, binning
    + xyz stage control: position, status (moving or not), move, jog
    + Live view: a live stream viewer, with overlay layers (cross hair, ROI indicator, profile, false color, saturation indication, histogram, etc)
  - `Scan Workflow`: Histo scan, snake scan, raster scan, z-scan, etc
  - `Workspace`: this is where we process the raw data, analyze the processed data, generate reports.
  - `Gallery`: every scanned dataset along with its metadata are saved to the filesystem as project and displayed in the Gallery. A card style item can be displayed here.
  - `Settings`: consists settings for System, display, camera, stage, calibration management, calibration wizard, simulated samples settings, simulated samples load/unload
  - `Jobs`: all the long-last jobs and their status listed here. Long-last jobs examples: scanning, auto exposure, etc.
  - `Logging`: info, warn, error, critical, etc.
  - `About`: about the software
  - `Power`: restart the service, reboot the host machine, shutdown
  - `Insturment Status Indicator`: the system status and health condition
* Soslving the streaming remotely problem:
  - For the local desktop app (Tauri 2): utlize local zero-copy file sharing or high-speed local memory references to load deep-zoom tiles or live views.
  - For the remote web browsers: Live preview + deep zoom
* Use VTK.js to render 3D volumes/surfaces dynamically on the client side, while letting the Rust backend handle tile-serving and data slicing over HTTP/WebSockets.
* Platforms supported: Linux and Windows
* Hardware support:
  - HAL
  - Real: drivers for the real hardware
  - Sim: drivers for simulated hardware for each of the real hardware, respectively.
* Simulated samples
  - general requirements: the software must run without real hardware available, using simulated hardware. When simulated hardware are used, simulated samples are used to support all workflows.
  - Simulated cells for phathological analysis, like: https://www.tucsen.com/uploads/xw-3.jpg
  - Simulated cells for confocal microscopy, like: https://www.tucsen.com/uploads/mic01.jpg
  - Simulated cells for super-resolution microscopy, like: https://www.tucsen.com/uploads/xw-1.jpg
* I want to implement the software in three milestones:
  - Milestone 1: focus on the Rust backend with multiple phases. In each phase, examples are required, simple front example if needed.
  - Milestone 2: focus on the Web frontend with multiple phases. In each phase, simulated hardware and samples are used.
  - Milestone 3: focus on integrate with real hardware and performance fine tune.
