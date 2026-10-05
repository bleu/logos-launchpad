import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Item {
    id: root
    width: 480
    height: 320

    property bool pending: false
    property string statusMessage: "Ready to test the Rust backend."

    function testBackend() {
        if (pending)
            return

        pending = true
        statusMessage = "Waiting for the backend..."
        try {
            logos.callModuleAsync("launchpad_core", "get_status", [], function(payload) {
                root.pending = false
                try {
                    var reply = JSON.parse(payload)
                    if (reply !== null && typeof reply === "object" && typeof reply.error === "string") {
                        root.statusMessage = "Call error: " + reply.error
                        if (typeof reply.message === "string")
                            root.statusMessage += " (" + reply.message + ")"
                        console.error(root.statusMessage)
                        return
                    }
                    if (typeof reply !== "string" || reply !== "Backend OK")
                        throw new Error("Expected Backend OK; received " + payload)
                    root.statusMessage = reply
                } catch (error) {
                    root.statusMessage = "Reply error: " + error.message
                    console.error(root.statusMessage)
                }
            }, 5000)
        } catch (error) {
            pending = false
            statusMessage = "Call error: " + error.message
            console.error(statusMessage)
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        Label {
            Layout.fillWidth: true
            text: "Launchpad admin"
            font.pixelSize: 22
            wrapMode: Text.Wrap
        }

        Label {
            Layout.fillWidth: true
            text: "Namespace administration is not available yet. Only the backend check works."
            wrapMode: Text.Wrap
        }

        Label {
            Layout.fillWidth: true
            text: root.statusMessage
            wrapMode: Text.Wrap
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }

        Button {
            text: "Test backend"
            enabled: !root.pending
            onClicked: root.testBackend()
        }

        BusyIndicator {
            running: root.pending
            visible: running
        }

        Item {
            Layout.fillHeight: true
        }
    }
}
