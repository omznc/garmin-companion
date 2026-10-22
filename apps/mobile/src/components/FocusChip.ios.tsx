// The date focus control on iOS. SwiftUI buttons, a native sheet, and native date pickers.
// `FocusAction` sits in a stack header and opens the sheet. `FocusRow` sits in a body and only clears.
import {
  BottomSheet,
  Button,
  DatePicker,
  Form,
  Group,
  Host,
  HStack,
  Menu,
  Section,
  Text,
} from "@expo/ui/swift-ui";
import { accessibilityLabel, labelStyle, presentationDetents } from "@expo/ui/swift-ui/modifiers";
import { Stack } from "expo-router";
import { useState } from "react";
import { View } from "react-native";
import {
  clearFocus,
  focusPresets,
  isoDay,
  makeFocus,
  setFocus,
  todayIso,
  useFocus,
} from "../data/focus";
import { chipState } from "../data/focusChip";

const noon = (iso: string) => new Date(`${iso}T12:00:00`);

function Sheet({ onClose }: { onClose: () => void }) {
  const focus = useFocus();
  const today = todayIso();
  const from = focus?.from ?? today;
  const to = focus?.to ?? today;
  const range = { end: noon(today) };
  // Each change applies at once. `makeFocus` puts the two dates in order.
  const pick = (a: string, b: string) => setFocus(makeFocus(a, b, today));
  return (
    <Group modifiers={[presentationDetents(["medium"])]}>
      <Form>
        <Section>
          <DatePicker
            title="From"
            selection={noon(from)}
            range={range}
            onDateChange={(d) => pick(isoDay(d), to)}
          />
          <DatePicker
            title="To"
            selection={noon(to)}
            range={range}
            onDateChange={(d) => pick(from, isoDay(d))}
          />
        </Section>
        <Section>
          <Menu label="Quick ranges" systemImage="calendar">
            {focusPresets(today).map((p) => (
              <Button
                key={p.label}
                label={p.label}
                onPress={() => {
                  setFocus(p.focus);
                  onClose();
                }}
              />
            ))}
          </Menu>
          <Button label="Done" onPress={onClose} />
        </Section>
      </Form>
    </Group>
  );
}

function ClearButton() {
  return (
    <Button
      systemImage="xmark.circle.fill"
      onPress={clearFocus}
      modifiers={[labelStyle("iconOnly"), accessibilityLabel("Clear date focus")]}
    />
  );
}

function ActionButtons({ onOpen }: { onOpen: () => void }) {
  const { live, label } = chipState(useFocus(), todayIso());
  return live ? (
    <Button
      systemImage="calendar"
      onPress={onOpen}
      modifiers={[labelStyle("iconOnly"), accessibilityLabel("Choose dates")]}
    />
  ) : (
    <HStack spacing={12}>
      <Button label={label} systemImage="calendar" onPress={onOpen} />
      <ClearButton />
    </HStack>
  );
}

/** The header action: a calendar button, or the focus label with its clear button. */
export function FocusAction() {
  const [open, setOpen] = useState(false);
  return (
    <View style={{ minHeight: 44, minWidth: 44, justifyContent: "center" }}>
      <Host matchContents>
        <BottomSheet
          isPresented={open}
          onIsPresentedChange={setOpen}
          anchor={<ActionButtons onOpen={() => setOpen(true)} />}
        >
          <Sheet onClose={() => setOpen(false)} />
        </BottomSheet>
      </Host>
    </View>
  );
}

/** The right header item that shows `FocusAction`. Put it in a screen. */
export const FocusHeader = () => (
  <Stack.Toolbar placement="right">
    <Stack.Toolbar.View>
      <FocusAction />
    </Stack.Toolbar.View>
  </Stack.Toolbar>
);

/** The chip row at the top of a body. It shows nothing for the live focus. */
export function FocusRow() {
  const { live, label } = chipState(useFocus(), todayIso());
  if (live) return null;
  return (
    <View style={{ minHeight: 44, paddingHorizontal: 16, alignItems: "flex-start" }}>
      <Host matchContents>
        <HStack spacing={12}>
          <Text>{label}</Text>
          <ClearButton />
        </HStack>
      </Host>
    </View>
  );
}
