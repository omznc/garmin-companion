// The date focus control on iOS. SwiftUI buttons, a native sheet, and native date pickers.
// `FocusHeader` puts its buttons in a stack header and opens the sheet. `FocusRow` sits in a body and only clears.
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
import { type ReactNode, useState } from "react";
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

/**
 * The right header items: a calendar button, or the focus label with a clear button, then
 * `children` (more toolbar items). Native toolbar buttons: a SwiftUI host in the header drew
 * an empty pill. The sheet sits in a zero-size host in the body.
 */
export function FocusHeader({ children }: { children?: ReactNode }) {
  const [open, setOpen] = useState(false);
  const { live, label } = chipState(useFocus(), todayIso());
  return (
    <>
      <Stack.Toolbar placement="right">
        {live ? (
          <Stack.Toolbar.Button
            icon="calendar"
            accessibilityLabel="Choose dates"
            onPress={() => setOpen(true)}
          />
        ) : (
          <Stack.Toolbar.Button accessibilityLabel="Choose dates" onPress={() => setOpen(true)}>
            {label}
          </Stack.Toolbar.Button>
        )}
        <Stack.Toolbar.Button
          icon="xmark.circle.fill"
          accessibilityLabel="Clear date focus"
          hidden={live}
          onPress={clearFocus}
        />
        {children}
      </Stack.Toolbar>
      <Host style={{ width: 0, height: 0 }}>
        <BottomSheet isPresented={open} onIsPresentedChange={setOpen}>
          <Sheet onClose={() => setOpen(false)} />
        </BottomSheet>
      </Host>
    </>
  );
}

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
