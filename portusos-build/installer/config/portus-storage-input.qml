// SPDX-License-Identifier: Apache-2.0
import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import Qt.labs.folderlistmodel 2.15
import io.calamares.core 1.0
import io.calamares.ui 1.0

Item {
    id: page
    property bool activatedInCalamares: false
    property bool armed: false

    function clearRuntimeInputs() {
        Global.remove("portusStorageInputArmed")
        Global.remove("portusTargetDevice")
        Global.remove("portusStoragePlanHash")
        Global.remove("portusOwnerLuksPassphrase")
        Global.remove("portusRecoveryCredential")
    }

    function selectedTarget() {
        if (targetBox.currentIndex < 0 || targetBox.currentText.length === 0)
            return ""
        return "/dev/" + targetBox.currentText
    }

    function formValid() {
        return selectedTarget().length > 0
            && ownerField.text.length > 0
            && ownerField.text === ownerConfirmField.text
            && recoveryField.text.length > 0
            && recoveryField.text === recoveryConfirmField.text
            && ownerField.text !== recoveryField.text
            && eraseCheck.checked
    }

    function onActivate() {
        armed = false
        clearRuntimeInputs()
        ownerField.text = ""
        ownerConfirmField.text = ""
        recoveryField.text = ""
        recoveryConfirmField.text = ""
        eraseCheck.checked = false
        statusLabel.text = targetModel.count === 0
            ? "No supported whole-disk device is visible under /sys/block. Cancel the installer and inspect storage."
            : "Select the installation disk and enter independent storage credentials."
    }

    function onLeave() {
        if (!armed)
            clearRuntimeInputs()
    }

    function commitAndContinue() {
        if (!formValid()) {
            statusLabel.text = "Complete the disk selection, matching credential confirmations, and erase confirmation first."
            return
        }
        clearRuntimeInputs()
        Global.insert("portusTargetDevice", selectedTarget())
        Global.insert("portusOwnerLuksPassphrase", ownerField.text)
        Global.insert("portusRecoveryCredential", recoveryField.text)
        Global.insert("portusStorageInputArmed", true)
        armed = true
        ViewManager.next()
    }

    FolderListModel {
        id: targetModel
        folder: "file:///sys/block"
        showDirs: true
        showFiles: false
        showDotAndDotDot: false
        nameFilters: ["sd*", "vd*", "xvd*", "nvme*n*", "mmcblk*", "hd*"]
        sortField: FolderListModel.Name
    }

    ScrollView {
        anchors.fill: parent
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 12

            Label {
                Layout.fillWidth: true
                text: "Storage & Recovery"
                font.pixelSize: 24
                font.bold: true
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: "PortusOS uses the entire selected disk: GPT, 512 MiB EFI System Partition, 2 GiB unencrypted /boot, then LUKS2 containing VG portus with an ext4 root LV, 4 GiB swap LV, and about 5% free VG reserve."
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: "The owner unlock credential and recovery credential must be different. Neither is reused as the Master Portus or root account password."
            }

            ComboBox {
                id: targetBox
                Layout.fillWidth: true
                model: targetModel
                textRole: "fileName"
                currentIndex: targetModel.count > 0 ? 0 : -1
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: selectedTarget().length === 0
                    ? "No supported target is selected."
                    : "Selected target: " + selectedTarget() + ". Before any write, PortusOS revalidates that it is a writable, unmounted whole block device of at least 40 GiB and computes a size-bound confirmation hash for the locked plan."
            }

            TextField {
                id: ownerField
                Layout.fillWidth: true
                placeholderText: "Owner LUKS unlock credential"
                echoMode: TextInput.Password
            }
            TextField {
                id: ownerConfirmField
                Layout.fillWidth: true
                placeholderText: "Confirm owner LUKS unlock credential"
                echoMode: TextInput.Password
            }
            TextField {
                id: recoveryField
                Layout.fillWidth: true
                placeholderText: "Independent recovery credential"
                echoMode: TextInput.Password
            }
            TextField {
                id: recoveryConfirmField
                Layout.fillWidth: true
                placeholderText: "Confirm independent recovery credential"
                echoMode: TextInput.Password
            }

            CheckBox {
                id: eraseCheck
                Layout.fillWidth: true
                text: selectedTarget().length === 0
                    ? "No target selected"
                    : "I understand that all data on " + selectedTarget() + " will be erased."
            }

            Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: "#b00020"
                text: "Use the Continue button on this page to authorize these storage inputs. The standard Calamares Next control is not treated as destructive authorization; bypassing this page makes the non-destructive preflight fail before disk writes begin."
            }

            Button {
                text: "Continue with this storage plan"
                enabled: page.formValid()
                onClicked: page.commitAndContinue()
            }

            Label {
                id: statusLabel
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
            }
        }
    }
}
