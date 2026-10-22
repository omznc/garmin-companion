// Sleep debt, bed and wake times and the sleep scores of the last nights, on Android.
import { Bars, Card, SectionLabel, T, TileGrid } from "./MdKit";
import type { SleepExtras as Extras } from "../data/viewsPresent";
import { useMd } from "../theme/md";

export function SleepExtras({ extras }: { extras: Extras }) {
  const md = useMd();
  const tiles = [
    ...(extras.debt ? [{ label: "Sleep debt", value: extras.debt, accent: true }] : []),
    ...extras.facts.map(([label, value]) => ({ label, value })),
  ];
  const { scores } = extras;
  return (
    <>
      {tiles.length > 0 ? (
        <>
          <SectionLabel>Sleep rhythm</SectionLabel>
          <TileGrid tiles={tiles} />
        </>
      ) : null}
      {scores ? (
        <>
          <SectionLabel>Sleep score</SectionLabel>
          <Card gap={12}>
            <Bars
              values={scores.points}
              max={100}
              height={96}
              gap={4}
              describe={(v, i) =>
                v === null ? `Night ${i + 1}: no score` : `Night ${i + 1}: ${v}`
              }
            />
            <T v="bodyMedium" color={md.onSurfaceVariant}>
              {scores.average ? `Average ${scores.average}` : scores.label}
            </T>
          </Card>
        </>
      ) : null}
    </>
  );
}
