import QtQuick
import QtMultimedia

// Loaded in isolation from the main shell so a missing QtMultimedia backend
// cannot take the rest of the shell down with it.
Item {
  id: preview

  property string device: ""
  property bool active: false
  property bool cameraRunning: false
  // Stays false until the camera has actually delivered a frame, so the panel
  // can keep a placeholder in place while the device warms up.
  property bool ready: false

  onDeviceChanged: {
    preview.ready = false
    if (preview.active) {
      preview.cameraRunning = false
      startTimer.restart()
    }
  }

  onActiveChanged: {
    if (active) startTimer.restart()
    else {
      startTimer.stop()
      preview.cameraRunning = false
      preview.ready = false
    }
  }

  Timer {
    id: startTimer
    interval: 1
    repeat: false
    onTriggered: preview.cameraRunning = preview.active
  }

  MediaDevices { id: mediaDevices }

  CaptureSession {
    videoOutput: output
    camera: Camera {
      active: preview.cameraRunning
      cameraDevice: {
        var devices = mediaDevices.videoInputs || []
        for (var i = 0; i < devices.length; i++) {
          if (preview.device !== "" && String(devices[i].id) === preview.device) return devices[i]
        }
        return mediaDevices.defaultVideoInput
      }
    }
  }

  VideoOutput {
    id: output
    anchors.fill: parent
    fillMode: VideoOutput.PreserveAspectCrop
    visible: preview.ready
  }

  Connections {
    target: output.videoSink
    function onVideoFrameChanged(frame) {
      if (preview.active) preview.ready = true
    }
  }
}
