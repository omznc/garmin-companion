// The chat history on Android: a Material 3 group of rows, a pull to refresh, and a
// "New chat" extended FAB. A long press renames or deletes a chat.
import MaterialIcons from "@expo/vector-icons/MaterialIcons";
import { useRouter } from "expo-router";
import { useState } from "react";
import { Pressable, RefreshControl, ScrollView, Text, View } from "react-native";
import { Card, Group, ListRow, T } from "../components/MdKit";
import { useMd } from "../theme/md";
import { AppBar } from "./AppBar";
import { RenameDialog } from "./RenameDialog";
import { Spinner } from "./Spinner";
import { Skeleton } from "./Skeleton";
import { showThreadMenu } from "./threadMenu";
import { useRenaming } from "./useRenaming";
import { useThreadRows } from "./useThreads";
import { whenOf } from "./when";

function NewChatFab({ onPress }: { onPress: () => void }) {
  const md = useMd();
  return (
    <View pointerEvents="box-none" style={{ position: "absolute", right: 16, bottom: 16 }}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="New chat"
        onPress={onPress}
        android_ripple={{ color: md.onPrimaryContainer }}
        style={{
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          height: 56,
          paddingHorizontal: 20,
          borderRadius: 16,
          overflow: "hidden",
          backgroundColor: md.primaryContainer,
          elevation: 3,
        }}
      >
        <MaterialIcons name="add" size={24} color={md.onPrimaryContainer as string} />
        <Text style={{ color: md.onPrimaryContainer, fontSize: 14, fontWeight: "500" }}>
          New chat
        </Text>
      </Pressable>
    </View>
  );
}

function SkeletonRows() {
  return (
    <Group>
      {[0, 1, 2].map((i) => (
        <View key={i} style={{ padding: 20, gap: 8 }}>
          <Skeleton height={16} width="60%" />
          <Skeleton height={12} width="30%" />
        </View>
      ))}
    </Group>
  );
}

type Rows = ReturnType<typeof useThreadRows>;

// The rows are not here yet, did not load, or there are none.
function Status({ list }: { list: Rows }) {
  const md = useMd();
  if (list.loading) return <SkeletonRows />;
  if (list.error) {
    return (
      <Card tone="error">
        <T color={md.onErrorContainer}>{`The chats could not be read. ${list.error.message}`}</T>
      </Card>
    );
  }
  if (list.rows.length > 0) return null;
  return (
    <Card tone="secondary">
      <T v="body" color={md.onSecondaryContainer}>
        No chats yet. Start one, and it shows here.
      </T>
    </Card>
  );
}

export function ThreadList() {
  const md = useMd();
  const router = useRouter();
  const list = useThreadRows();
  const rename = useRenaming();
  const [now] = useState(Date.now);
  const start = () => router.push("/ask/new");

  return (
    <View style={{ flex: 1, backgroundColor: md.surface }}>
      <AppBar title="Chats" back />
      <ScrollView
        refreshControl={<RefreshControl refreshing={list.refreshing} onRefresh={list.onRefresh} />}
        contentContainerStyle={{ padding: 16, paddingBottom: 96, gap: 12 }}
      >
        <Status list={list} />
        {list.rows.length > 0 ? (
          <Group>
            {list.rows.map((thread) => (
              <ListRow
                key={thread.sessionId}
                icon="chat-bubble-outline"
                title={thread.title}
                supporting={thread.running ? "Working" : whenOf(thread.updatedAt, now)}
                trailing={
                  thread.running ? (
                    <Spinner size={24} color={md.primary} label="Working" />
                  ) : undefined
                }
                onPress={() => router.push(`/ask/${thread.sessionId}`)}
                onLongPress={() => showThreadMenu(thread, () => rename.begin(thread))}
              />
            ))}
          </Group>
        ) : null}
      </ScrollView>
      <NewChatFab onPress={start} />
      <RenameDialog
        // A new key gives each rename a fresh field.
        key={rename.renaming?.sessionId ?? "closed"}
        initial={rename.renaming?.title ?? null}
        onCancel={rename.cancel}
        onSave={rename.save}
      />
    </View>
  );
}
