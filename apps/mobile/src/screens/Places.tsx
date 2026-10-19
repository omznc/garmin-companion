// Plan, Routes and Gear. One screen for the three: each is a list of blocks from `data/places.ts`.
import { type PlacesKind, usePlaces } from "../data/places";
import { BlocksScreen } from "./Body";

export function PlacesScreen({ kind, title }: { kind: PlacesKind; title: string }) {
  return <BlocksScreen title={title} query={usePlaces(kind)} />;
}
