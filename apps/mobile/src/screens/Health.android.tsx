// Health on Android: a Sleep card that opens the Sleep detail, then one block for each metric.
// The core sends the metrics that the watch records, with a trend sentence for each.
import { Card, Column, Row, Text } from "@expo/ui/jetpack-compose";
import { clickable, fillMaxWidth, paddingAll, weight } from "@expo/ui/jetpack-compose/modifiers";
import { useRouter } from "expo-router";
import { View } from "react-native";
import { BodyLinks } from "../components/BodyLinks";
import { LargeAppBarScreen } from "../components/LargeAppBarScreen";
import { Block, Body, Caption, ErrorBlock, Heading, Loading } from "../components/MdBlocks";
import { MiniBars } from "../components/MiniBars";
import { type MetricTrend, scaled, useHealth, useSleep } from "../data/wellness";
import { useMd } from "../theme/md";

type Md = ReturnType<typeof useMd>;

function SleepCard({ md }: { md: Md }) {
  const router = useRouter();
  const { data } = useSleep();
  return (
    <Block>
      <Card
        colors={{ containerColor: md.surfaceContainerHigh }}
        modifiers={[fillMaxWidth(), clickable(() => router.push("/health/sleep"))]}
      >
        <Column verticalArrangement={{ spacedBy: 8 }} modifiers={[fillMaxWidth(), paddingAll(16)]}>
          <Text style={{ typography: "titleMedium" }} color={md.onSurface}>
            Sleep
          </Text>
          <Text style={{ typography: "bodyMedium" }} color={md.onSurfaceVariant}>
            {data?.lede ?? "Last night, the stages and the last 30 nights."}
          </Text>
        </Column>
      </Card>
    </Block>
  );
}

function MetricBlock({ md, m }: { md: Md; m: MetricTrend }) {
  const value = m.unit ? `${m.valueText} ${m.unit}` : m.valueText;
  return (
    <>
      <Block>
        <Row modifiers={[fillMaxWidth()]}>
          <Column modifiers={[weight(1)]}>
            <Heading md={md}>{m.label}</Heading>
          </Column>
          <Text style={{ typography: "headlineSmall" }} color={md.onSurface}>
            {value}
          </Text>
        </Row>
      </Block>
      <MiniBars
        values={scaled(m.points)}
        max={1}
        height={40}
        color={md.primary}
        trackColor={md.surfaceVariant}
        describe={(v, i) =>
          v === null ? `Day ${i + 1}: no reading` : `Day ${i + 1} of ${m.points.length}`
        }
      />
      <Block>
        <Body md={md}>{m.sentence}</Body>
        <Caption md={md}>{m.caption}</Caption>
      </Block>
    </>
  );
}

function HealthBody() {
  const md = useMd();
  const { data, error, refetch } = useHealth();
  if (error) return <ErrorBlock message={String(error)} onRetry={refetch} />;
  if (!data) return <Loading />;
  return (
    <>
      <Block>
        <Body md={md}>{data.note}</Body>
      </Block>
      <SleepCard md={md} />
      <BodyLinks />
      {data.metrics.map((m) => (
        <MetricBlock key={m.key} md={md} m={m} />
      ))}
      <View style={{ height: 32 }} />
    </>
  );
}

export function HealthScreen() {
  return (
    <LargeAppBarScreen title="Health">
      <HealthBody />
    </LargeAppBarScreen>
  );
}
