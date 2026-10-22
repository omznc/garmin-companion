// The date focus control on Android. M3 assist chip with a clear button, and the M3 range picker.
// `FocusAction` sits in a top app bar and opens the picker. `FocusRow` sits in a body and only clears.
import { Host } from "@expo/ui";
import {
  AssistChip,
  DateRangePickerDialog,
  Icon,
  IconButton,
  Row,
  Text,
} from "@expo/ui/jetpack-compose";
import { useState } from "react";
import { View } from "react-native";
import { clearFocus, setFocus, todayIso, useFocus } from "../data/focus";
import { chipState, pickedFocus, utcDay } from "../data/focusChip";
import { useHostTheme, useMd } from "../theme/md";

// Local noon: the picker reads the local day of the instant, so a UTC midnight is the day before west of UTC.
const utcMidnight = (iso: string) => new Date(`${iso}T12:00:00`);

function Picker({ onClose }: { onClose: () => void }) {
  const focus = useFocus();
  const today = todayIso();
  return (
    <DateRangePickerDialog
      initialStartDate={focus ? utcMidnight(focus.from).toISOString() : null}
      initialEndDate={focus ? utcMidnight(focus.to).toISOString() : null}
      selectableDates={{ end: utcMidnight(today) }}
      showVariantToggle={false}
      onDismissRequest={onClose}
      onDateRangeSelected={(range) => {
        if (!range.start || !range.end) return;
        setFocus(pickedFocus(utcDay(range.start), utcDay(range.end), today));
        onClose();
      }}
    />
  );
}

function ClearButton() {
  const md = useMd();
  return (
    <IconButton onClick={clearFocus}>
      <Icon
        source={require("../../assets/icons/close.xml")}
        size={20}
        tint={md.onSurfaceVariant}
        contentDescription="Clear date focus"
      />
    </IconButton>
  );
}

/** The header action: a calendar button, or the focus chip with its clear button. */
export function FocusAction() {
  const md = useMd();
  const host = useHostTheme();
  const [open, setOpen] = useState(false);
  const { live, label } = chipState(useFocus(), todayIso());
  return (
    <View style={{ minHeight: 48, justifyContent: "center" }}>
      <Host matchContents {...host}>
        <Row verticalAlignment="center">
          {live ? (
            <IconButton onClick={() => setOpen(true)}>
              <Icon
                source={require("../../assets/icons/calendar.xml")}
                size={24}
                tint={md.onSurface}
                contentDescription="Choose dates"
              />
            </IconButton>
          ) : (
            <>
              <AssistChip onClick={() => setOpen(true)}>
                <AssistChip.Label>
                  <Text>{label}</Text>
                </AssistChip.Label>
              </AssistChip>
              <ClearButton />
            </>
          )}
          {open ? <Picker onClose={() => setOpen(false)} /> : null}
        </Row>
      </Host>
    </View>
  );
}

/** Android draws `FocusAction` in its own app bar, so the header item is empty. */
export const FocusHeader = () => null;

/** The chip row at the top of a body. It shows nothing for the live focus. */
export function FocusRow() {
  const host = useHostTheme();
  const { live, label } = chipState(useFocus(), todayIso());
  if (live) return null;
  return (
    <View style={{ minHeight: 48, paddingHorizontal: 16, alignItems: "flex-start" }}>
      <Host matchContents {...host}>
        <Row verticalAlignment="center">
          <AssistChip>
            <AssistChip.Label>
              <Text>{label}</Text>
            </AssistChip.Label>
          </AssistChip>
          <ClearButton />
        </Row>
      </Host>
    </View>
  );
}
