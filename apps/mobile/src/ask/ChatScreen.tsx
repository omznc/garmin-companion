// One chat, for the draft and for a saved thread. The draft has the opening and no
// session. The first message makes the session, and this same screen goes on with it.
import { useRouter } from "expo-router";
import { type ComponentRef, useEffect, useRef, useState } from "react";
import {
  Keyboard,
  KeyboardAvoidingView,
  type LayoutChangeEvent,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  Platform,
  ScrollView,
  Text,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { needsSetup, openers, useCoachConfig } from "../data/useAsk";
import { useCloudAccount } from "../data/useCloud";
import { ChatHeader } from "./ChatHeader";
import { Composer } from "./Composer";
import { CreditLine } from "./CreditLine";
import { EmptyHero } from "./EmptyHero";
import { Setup } from "./Setup";
import { Skeleton } from "./Skeleton";
import { TurnView } from "./TurnView";
import { useRunningId, stopTurn } from "./runs";
import { useChatTheme } from "./theme";
import { newSessionId } from "./turns";
import { sendMessage, useThread } from "./useThreads";

// Closer to the end than this, the chat follows a new answer.
const PIN_DISTANCE = 96;

const EMPTY_BODY =
  "Ask about your sleep, recovery, activity or weight. The coach reads your own Garmin data before it answers.";

// True while the keyboard is open, to drop the bottom inset under it.
function useKeyboardOpen() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const ios = Platform.OS === "ios";
    const show = Keyboard.addListener(ios ? "keyboardWillShow" : "keyboardDidShow", () =>
      setOpen(true),
    );
    const hide = Keyboard.addListener(ios ? "keyboardWillHide" : "keyboardDidHide", () =>
      setOpen(false),
    );
    return () => {
      show.remove();
      hide.remove();
    };
  }, []);
  return open;
}

// Follows a growing answer only while the person is near the end. A drag lets go of
// the pin, and a scroll back near the end takes it again.
function usePinnedScroll() {
  const scroll = useRef<ComponentRef<typeof ScrollView>>(null);
  const pinned = useRef(true);
  const size = useRef({ content: 0, layout: 0 });
  // A short thread does not scroll. scrollToEnd then goes to offset 0, and on iOS
  // that hides the top inset under the header.
  const toEnd = () => {
    const { content, layout } = size.current;
    if (pinned.current && content > layout) scroll.current?.scrollToEnd({ animated: false });
  };
  return {
    scroll,
    pin: () => {
      pinned.current = true;
    },
    onScrollBeginDrag: () => {
      pinned.current = false;
    },
    onScroll: (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
      if (contentSize.height - contentOffset.y - layoutMeasurement.height < PIN_DISTANCE) {
        pinned.current = true;
      }
    },
    // The content grows, or the list gets shorter under the keyboard.
    onContentSizeChange: (_: number, height: number) => {
      size.current.content = height;
      toEnd();
    },
    onLayout: (event: LayoutChangeEvent) => {
      size.current.layout = event.nativeEvent.layout.height;
      toEnd();
    },
  };
}

// The first message of a draft makes the session. The draft then goes on with it.
function useSend(id: string | null, onStarted: (sessionId: string) => void, pin: () => void) {
  const sending = useRef(false);
  return async (text: string) => {
    if (sending.current) return false;
    sending.current = true;
    pin();
    try {
      const target = id ?? newSessionId();
      const landed = await sendMessage(target, text);
      if (landed && id === null) onStarted(target);
      return landed;
    } finally {
      sending.current = false;
    }
  };
}

type Thread = ReturnType<typeof useThread>;

function Transcript({
  id,
  thread,
  onPick,
}: {
  id: string | null;
  thread: Thread;
  onPick: (question: string) => void;
}) {
  const theme = useChatTheme();
  const config = useCoachConfig();
  const [suggestions] = useState(openers);
  const runningId = useRunningId();
  const { turns, loading, error, refetch } = thread;
  const showHero = id === null || (!loading && !error && turns.length === 0);
  return (
    <>
      {loading ? (
        <View style={{ gap: 12 }}>
          <Skeleton height={48} width="60%" />
          <Skeleton height={120} />
        </View>
      ) : null}
      {error ? (
        <Text
          accessibilityRole="button"
          onPress={() => void refetch()}
          style={{ color: theme.error, fontSize: 14, lineHeight: 20 }}
        >
          {`The chat could not be read. ${error.message} Tap to try again.`}
        </Text>
      ) : null}
      {showHero ? (
        <View>
          <EmptyHero
            body={EMPTY_BODY}
            suggestions={suggestions}
            onPick={onPick}
            locked={runningId !== null && runningId !== id}
          />
          {id === null && needsSetup(config.data) ? (
            <Setup onSaved={() => void config.refetch()} />
          ) : null}
        </View>
      ) : null}
      {turns.map((turn) => (
        <TurnView key={turn.id} turn={turn} />
      ))}
    </>
  );
}

function Credits() {
  const router = useRouter();
  const config = useCoachConfig();
  const account = useCloudAccount(config.data?.provider === "cloud").data;
  if (!account || config.data?.provider !== "cloud") return null;
  return <CreditLine account={account} onOwnKey={() => router.push("/more/coach" as never)} />;
}

// The credits and the composer. A turn of another chat locks the field.
function Footer({
  id,
  root,
  keyboardOpen,
  onSend,
}: {
  id: string | null;
  root: boolean;
  keyboardOpen: boolean;
  onSend: (text: string) => Promise<boolean>;
}) {
  const runningId = useRunningId();
  const streaming = id !== null && runningId === id;
  return (
    <>
      <Credits />
      {/* iOS: the native tab bar floats over the screen, and only the view's own safe area
          includes it. Android: the tab bar sits below the screen and owns the system inset. */}
      <SafeAreaView edges={Platform.OS === "ios" && !keyboardOpen ? ["bottom"] : []}>
        <Composer
          onSend={onSend}
          onStop={stopTurn}
          streaming={streaming}
          locked={runningId !== null && !streaming}
          lockedReason="Another chat is answering"
          bottomInset={keyboardOpen ? 8 : 12}
          placeholder="Ask about your sleep, training or recovery"
          autoFocus={!root && id === null}
        />
      </SafeAreaView>
    </>
  );
}

export function ChatScreen({
  routeId,
  root = false,
}: {
  /** The saved thread of the route, or null for a draft. */
  routeId: string | null;
  /** The draft that is the root of the Ask tab. */
  root?: boolean;
}) {
  const theme = useChatTheme();
  const router = useRouter();
  // A draft whose first message went out. The same screen goes on with that thread, so no
  // screen slides in and Back goes where it went before.
  const [startedId, setStartedId] = useState<string | null>(null);
  const id = routeId ?? startedId;
  const thread = useThread(id);
  const pinned = usePinnedScroll();
  const send = useSend(id, setStartedId, pinned.pin);
  const frame = useRef<ComponentRef<typeof View>>(null);
  const [offset, setOffset] = useState(0);
  const keyboardOpen = useKeyboardOpen();
  // The thread of this draft was deleted: the root goes back to an empty draft.
  useEffect(() => {
    if (routeId === null && thread.gone) setStartedId(null);
  }, [routeId, thread.gone]);

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg }}>
      <ChatHeader
        title={id === null ? "Ask" : (thread.title ?? "New chat")}
        back={!root}
        onHistory={() => (root ? router.push("/ask/history") : router.dismissTo("/ask/history"))}
      />
      <View
        ref={frame}
        style={{ flex: 1 }}
        // The keyboard is measured against the window, and this view starts below the
        // header. Its own top is the offset.
        onLayout={() => frame.current?.measureInWindow((_, y) => setOffset(y))}
      >
        <KeyboardAvoidingView
          style={{ flex: 1 }}
          behavior="padding"
          keyboardVerticalOffset={offset}
        >
          <ScrollView
            ref={pinned.scroll}
            style={{ flex: 1 }}
            contentContainerStyle={{ padding: 16, gap: 24, flexGrow: 1 }}
            keyboardDismissMode="interactive"
            keyboardShouldPersistTaps="handled"
            contentInsetAdjustmentBehavior="always"
            scrollEventThrottle={64}
            onScrollBeginDrag={pinned.onScrollBeginDrag}
            onScroll={pinned.onScroll}
            onContentSizeChange={pinned.onContentSizeChange}
            onLayout={pinned.onLayout}
          >
            <Transcript id={id} thread={thread} onPick={(question) => void send(question)} />
          </ScrollView>
          <Footer id={id} root={root} keyboardOpen={keyboardOpen} onSend={send} />
        </KeyboardAvoidingView>
      </View>
    </View>
  );
}
