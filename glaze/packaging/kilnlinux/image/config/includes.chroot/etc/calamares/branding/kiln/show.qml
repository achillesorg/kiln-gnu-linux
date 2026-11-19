/* Kiln GNU/Linux installer slideshow (Calamares slideshowAPI 2).
 * Glaze Clay look: paper background, ink text, terracotta accents. Text only plus the Kiln logo, no external assets. */
import QtQuick 2.15
import calamares.slideshow 1.0

Presentation {
    id: presentation

    readonly property color paper: "#F5F2EA"
    readonly property color ink: "#2B2926"
    readonly property color terracotta: "#D97757"
    readonly property color line: "#CFC6B4"

    function nextSlide() { presentation.goToNextSlide(); }

    Timer {
        id: advanceTimer
        interval: 9000
        running: presentation.activatedInCalamares
        repeat: true
        onTriggered: nextSlide()
    }

    Rectangle {
        anchors.fill: parent
        color: presentation.paper
        z: -1
    }

    component KilnSlide: Slide {
        property string heading: ""
        property string body: ""

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: Math.min(parent.width - 48, 640)
            height: Math.min(parent.height - 48, 300)
            color: "#FBF9F4"
            border.color: presentation.ink
            border.width: 1
            radius: 6

            /* hard offset shadow, as Glaze popovers */
            Rectangle {
                z: -1
                x: 4; y: 4
                width: parent.width; height: parent.height
                radius: 6
                color: presentation.line
            }

            Image {
                id: mark
                source: "logo.png"
                width: 64; height: 64
                fillMode: Image.PreserveAspectFit
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.margins: 24
            }
            Text {
                id: title
                text: heading
                color: presentation.ink
                font.pixelSize: 24
                font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
                anchors.left: mark.right
                anchors.right: parent.right
                anchors.verticalCenter: mark.verticalCenter
                anchors.leftMargin: 16
                anchors.rightMargin: 24
            }
            Rectangle {
                id: rule
                height: 3; width: 48
                color: presentation.terracotta
                anchors.left: parent.left
                anchors.top: mark.bottom
                anchors.margins: 24
            }
            Text {
                text: body
                color: presentation.ink
                font.pixelSize: 15
                lineHeight: 1.25
                wrapMode: Text.WordWrap
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: rule.bottom
                anchors.margins: 24
            }
        }
    }

    KilnSlide {
        heading: "Welcome to Kiln GNU/Linux"
        body: "Kiln is copying itself to your disk. It is the same system you have been trying in the live session: Debian 13 underneath, the Glaze desktop on top."
    }
    KilnSlide {
        heading: "Glaze, your desktop"
        body: "Start menu, taskbar and windows you already know, in a flat cut-paper look. Change themes, the panel and the wallpaper in Glaze Settings."
    }
    KilnSlide {
        heading: "Apps included"
        body: "Kiln Files, Terminal, Text, Viewer, Calculator, Monitor and Firefox are ready to use. More apps come from Debian (apt) and Flathub through the Kiln Store."
    }
    KilnSlide {
        heading: "After the restart"
        body: "Log in on the first console with the user you just created: Glaze starts by itself. Your user can run administrative commands with sudo."
    }

    function onActivate() {
        presentation.currentSlide = 0;
    }
    function onLeave() {
    }
}
