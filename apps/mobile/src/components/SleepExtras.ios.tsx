// Sleep debt, bed and wake times and the sleep scores of the last nights, on iOS.
import { PlatformColor, View } from "react-native";
import type { SleepExtras as Extras } from "../data/viewsPresent";
import { Group, Note, Row } from "./Grouped";
import { MiniBars } from "./MiniBars";

export function SleepExtras({ extras }: { extras: Extras }) {
  const rows: [string, string][] = [
    ...(extras.debt ? ([["Sleep debt", extras.debt]] as [string, string][]) : []),
    ...extras.facts,
  ];
  const { scores } = extras;
  return (
    <>
      {rows.length > 0 ? (
        <Group title="Sleep rhythm">
          {rows.map(([left, right], i) => (
            <Row key={left} left={left} right={right} last={i === rows.length - 1} />
          ))}
        </Group>
      ) : null}
      {scores ? (
        <Group title="Sleep score">
          <View style={{ paddingTop: 16 }}>
            <MiniBars
              values={scores.points}
              max={100}
              height={96}
              gap={4}
              color={PlatformColor("systemIndigo")}
              trackColor={PlatformColor("separator")}
              describe={(v, i) =>
                v === null ? `Night ${i + 1}: no score` : `Night ${i + 1}: ${v}`
              }
            />
          </View>
          <Note>{scores.average ? `Average ${scores.average}` : scores.label}</Note>
        </Group>
      ) : null}
    </>
  );
}
