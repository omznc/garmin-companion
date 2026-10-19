// The used surface of the generated `react-native-garmin-bridge` module. The
// module comes from `pnpm bridge:android` or `pnpm bridge:ios` and is not in git.
// This file lets the typecheck run without a Rust build. Keep it equal to the
// exports of crates/garmin-ffi/src/lib.rs, in the ubrn naming (camelCase).
declare module "react-native-garmin-bridge" {
  export interface EventListener {
    onEvent(kind: string, payloadJson: string): void;
  }
  export interface Subscription {
    cancel(): void;
  }
  export function init(baseDir: string, fixtureMode: boolean): void;
  export function version(): string;
  export function call(cmd: string, argsJson: string): Promise<string>;
  export function subscribe(listener: EventListener): Subscription;
}
