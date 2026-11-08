#!/usr/bin/env python3
"""Builds and signs assets/st Focus.shortcut, the shortcut `st --focus` runs.

Run from the repo root on a Mac signed in to an Apple ID:

    python3 scripts/build-focus-shortcut.py

macOS has no public API for Focus, so st drives the Shortcuts app's Set Focus
action. The shortcut reads its input text: "on" turns Do Not Disturb on until
turned off, "off" turns it off, and anything else is an end time.
"""

import plistlib
import subprocess
import sys
import uuid
from pathlib import Path

NAME = "st Focus"
ASSETS = Path("assets")
UNSIGNED = ASSETS / f"{NAME}.unsigned.shortcut"
SIGNED = ASSETS / f"{NAME}.shortcut"
SIGN_ATTEMPTS = 3
SIGN_TIMEOUT_SECONDS = 60

COMMENT = (
    "Turns Do Not Disturb on or off for st, the status command line tool. "
    "st runs this by name, so keep it named st Focus."
)
FOCUS_MODES = {
    "Identifier": "com.apple.donotdisturb.mode.default",
    "DisplayString": "Do Not Disturb",
}


def new_id():
    return str(uuid.uuid4()).upper()


def action(identifier, **params):
    return {
        "WFWorkflowActionIdentifier": f"is.workflow.actions.{identifier}",
        "WFWorkflowActionParameters": params,
    }


def text_output(output_uuid):
    return {"Type": "ActionOutput", "OutputUUID": output_uuid, "OutputName": "Text"}


def if_text_is(group, text, text_uuid):
    return action(
        "conditional",
        GroupingIdentifier=group,
        WFControlFlowMode=0,
        WFCondition=4,
        WFConditionalActionString=text,
        WFInput={
            "Type": "Variable",
            "Variable": {
                "Value": text_output(text_uuid),
                "WFSerializationType": "WFTextTokenAttachment",
            },
        },
    )


def otherwise(group):
    return action("conditional", GroupingIdentifier=group, WFControlFlowMode=1)


def end_if(group):
    return action("conditional", GroupingIdentifier=group, WFControlFlowMode=2)


def workflow():
    text_uuid = new_id()
    outer, inner = new_id(), new_id()

    # Set Focus ignores a date value for Time, so the end time goes in as text
    until_time = {
        "WFSerializationType": "WFTextTokenString",
        "Value": {
            "string": "￼",
            "attachmentsByRange": {"{0, 1}": text_output(text_uuid)},
        },
    }

    actions = [
        action("comment", WFCommentActionText=COMMENT),
        action(
            "detect.text",
            UUID=text_uuid,
            WFInput={
                "Value": {"Type": "ExtensionInput"},
                "WFSerializationType": "WFTextTokenAttachment",
            },
        ),
        if_text_is(outer, "on", text_uuid),
        action(
            "dnd.set",
            Operation="Turn",
            Enabled=1,
            AssertionType="Turned Off",
            FocusModes=FOCUS_MODES,
        ),
        otherwise(outer),
        if_text_is(inner, "off", text_uuid),
        action("dnd.set", Operation="Turn", Enabled=0, FocusModes=FOCUS_MODES),
        otherwise(inner),
        action(
            "dnd.set",
            Operation="Turn",
            Enabled=1,
            AssertionType="Time",
            FocusModes=FOCUS_MODES,
            Time=until_time,
        ),
        end_if(inner),
        end_if(outer),
        # Without this the last step's output reaches stdout and macOS asks to allow it
        action("nothing"),
    ]

    return {
        "WFWorkflowClientVersion": "2612.0.4",
        "WFWorkflowMinimumClientVersion": 900,
        "WFWorkflowMinimumClientVersionString": "900",
        "WFWorkflowIcon": {
            "WFWorkflowIconStartColor": 463140863,
            "WFWorkflowIconGlyphNumber": 59511,
        },
        "WFWorkflowTypes": [],
        "WFWorkflowImportQuestions": [],
        "WFWorkflowInputContentItemClasses": [
            "WFStringContentItem",
            "WFGenericFileContentItem",
        ],
        "WFWorkflowActions": actions,
    }


def sign():
    for attempt in range(1, SIGN_ATTEMPTS + 1):
        try:
            subprocess.run(
                [
                    "shortcuts", "sign", "--mode", "anyone",
                    "--input", str(UNSIGNED), "--output", str(SIGNED),
                ],
                check=True,
                timeout=SIGN_TIMEOUT_SECONDS,
            )
            return True
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            # Apple's signing service sometimes returns HTTP 502
            print(f"Signing attempt {attempt} failed: {error}", file=sys.stderr)
    return False


def main():
    ASSETS.mkdir(exist_ok=True)
    with UNSIGNED.open("wb") as f:
        plistlib.dump(workflow(), f, fmt=plistlib.FMT_BINARY)

    if not sign():
        sys.exit(f"Couldn't sign {SIGNED} after {SIGN_ATTEMPTS} attempts")
    print(f"Wrote {SIGNED}")


if __name__ == "__main__":
    main()
