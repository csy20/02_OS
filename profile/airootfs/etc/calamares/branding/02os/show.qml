import QtQuick 2.5
import calamares.slideshow 1.0

Presentation {
    id: presentation

    function nextSlide() {
        presentation.goToNextSlide()
    }

    Timer {
        interval: 8000
        running: true
        repeat: true
        onTriggered: nextSlide()
    }

    Slide {
        anchors.fill: parent
        Image {
            source: "welcome.png"
            fillMode: Image.PreserveAspectCrop
            anchors.fill: parent
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 48
            text: "Welcome to 02_OS"
            color: "white"
            font.pixelSize: 28
            font.family: "Inter"
        }
    }

    Slide {
        anchors.fill: parent
        Rectangle { anchors.fill: parent; color: "#1c1c1e" }
        Text {
            anchors.centerIn: parent
            horizontalAlignment: Text.AlignHCenter
            text: "Hyprland desktop\nMac-like shell, built on Arch"
            color: "#f5f5f7"
            font.pixelSize: 24
            font.family: "Inter"
        }
    }
}
