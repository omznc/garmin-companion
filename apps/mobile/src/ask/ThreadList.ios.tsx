// The chat history on iOS: a grouped inset list under the large title, a pull to refresh,
// and a compose button in the header. A long press renames or deletes a chat.
import { Stack, useRouter } from "expo-router";
import { type ReactNode, useState } from "react";
import { FlatList, PlatformColor, Pressable, RefreshControl, Text, View } from "react-native";
import type { ThreadMeta } from "../data/chatTypes";
import { Group, label, secondary } from "../components/Grouped";
import { RenameDialog } from "./RenameDialog";
import { Skeleton } from "./Skeleton";
import { Spinner } from "./Spinner";
import { showThreadMenu } from "./threadMenu";
import { useRenaming } from "./useRenaming";
import { useThreadRows } from "./useThreads";
import { whenOf } from "./when";

const separator = PlatformColor("separator");

function ThreadRow({
  thread,
  now,
  last,
  onOpen,
  onMenu,
}: {
  thread: ThreadMeta;
  now: number;
  last: boolean;
  onOpen: () => void;
  onMenu: () => void;
}) {
  const sub = thread.running ? "Working" : whenOf(thread.updatedAt, now);
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${thread.title}, ${sub}`}
      onPress={onOpen}
      onLongPress={onMenu}
      style={({ pressed }) => ({
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        minHeight: 60,
        paddingHorizontal: 16,
        paddingVertical: 10,
        borderBottomWidth: last ? 0 : 0.5,
        borderBottomColor: separator,
        opacity: pressed ? 0.6 : 1,
      })}
    >
      <View style={{ flex: 1 }}>
        <Text numberOfLines={2} style={{ fontSize: 17, color: label }}>
          {thread.title}
        </Text>
        {sub ? <Text style={{ fontSize: 15, color: secondary }}>{sub}</Text> : null}
      </View>
      {thread.running ? <Spinner size={20} color={secondary} label="Working" /> : null}
    </Pressable>
  );
}

// A row of the inset group. The list draws it row by row, so the first and last row hold the corners.
function GroupCell({
  first,
  last,
  children,
}: {
  first: boolean;
  last: boolean;
  children: ReactNode;
}) {
  return (
    <View
      style={{
        marginTop: first ? 24 : 0,
        marginHorizontal: 16,
        backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
        borderTopLeftRadius: first ? 26 : 0,
        borderTopRightRadius: first ? 26 : 0,
        borderBottomLeftRadius: last ? 26 : 0,
        borderBottomRightRadius: last ? 26 : 0,
        overflow: "hidden",
      }}
    >
      {children}
    </View>
  );
}

function Message({ children }: { children: string }) {
  return (
    <View
      style={{
        margin: 16,
        padding: 16,
        borderRadius: 26,
        backgroundColor: PlatformColor("secondarySystemGroupedBackground"),
      }}
    >
      <Text style={{ fontSize: 15, color: secondary }}>{children}</Text>
    </View>
  );
}

type Rows = ReturnType<typeof useThreadRows>;

// The rows are not here yet, did not load, or there are none.
function Status({ list }: { list: Rows }) {
  if (list.loading) {
    return (
      <Group>
        {[0, 1, 2].map((i) => (
          <View key={i} style={{ padding: 16, gap: 8 }}>
            <Skeleton height={17} width="60%" />
            <Skeleton height={13} width="30%" />
          </View>
        ))}
      </Group>
    );
  }
  if (list.error) return <Message>{`The chats could not be read. ${list.error.message}`}</Message>;
  if (list.rows.length > 0) return null;
  return <Message>No chats yet. Start one, and it shows here.</Message>;
}

export function ThreadList() {
  const router = useRouter();
  const list = useThreadRows();
  const rename = useRenaming();
  const [now] = useState(Date.now);

  return (
    <>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          icon="square.and.pencil"
          accessibilityLabel="New chat"
          onPress={() => router.push("/ask/new")}
        />
      </Stack.Toolbar>
      <FlatList
        contentInsetAdjustmentBehavior="automatic"
        style={{ backgroundColor: PlatformColor("systemGroupedBackground") }}
        contentContainerStyle={{ paddingBottom: 32 }}
        refreshControl={<RefreshControl refreshing={list.refreshing} onRefresh={list.onRefresh} />}
        data={list.rows}
        keyExtractor={(thread) => thread.sessionId}
        ListHeaderComponent={<Status list={list} />}
        renderItem={({ item: thread, index }) => (
          <GroupCell first={index === 0} last={index === list.rows.length - 1}>
            <ThreadRow
              thread={thread}
              now={now}
              last={index === list.rows.length - 1}
              onOpen={() => router.push(`/ask/${thread.sessionId}`)}
              onMenu={() => showThreadMenu(thread, () => rename.begin(thread))}
            />
          </GroupCell>
        )}
      />
      <RenameDialog
        // A new key gives each rename a fresh field.
        key={rename.renaming?.sessionId ?? "closed"}
        initial={rename.renaming?.title ?? null}
        onCancel={rename.cancel}
        onSave={rename.save}
      />
    </>
  );
}
