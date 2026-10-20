// Garmin's own sign-in page in a web view. The app never sees the password.
// When the page goes to garmin.com with a ticket, the app stops it and redeems the ticket.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useRef, useState } from "react";
import { ActivityIndicator, Button, Text, useColorScheme, View } from "react-native";
import { WebView } from "react-native-webview";
import { getSignInUrl, redeemTicket } from "../src/data/account";
import { ticketIn } from "../src/data/ticket";
import { importHistory } from "../src/data/useSync";

type Step = "page" | "redeeming" | "failed";

function Message({ children, action }: { children: string; action?: React.ReactNode }) {
  const dark = useColorScheme() === "dark";
  return (
    <View
      style={{
        flex: 1,
        justifyContent: "center",
        padding: 24,
        gap: 16,
        backgroundColor: dark ? "#121212" : "#ffffff",
      }}
    >
      <ActivityIndicator animating={!action} />
      <Text style={{ fontSize: 16, textAlign: "center", color: dark ? "#ffffff" : "#000000" }}>
        {children}
      </Text>
      {action}
    </View>
  );
}

export default function SignIn() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const url = useQuery({ queryKey: ["sign-in-url"], queryFn: getSignInUrl, retry: false });
  const [step, setStep] = useState<Step>("page");
  const [problem, setProblem] = useState("");
  // The ticket works once, so only the first one counts.
  const used = useRef(false);

  const finish = () => router.replace("/today");

  const start = async (ticket: string) => {
    try {
      setStep("redeeming");
      const status = await redeemTicket(ticket);
      // Sample mode data must not outlive the sign-in.
      await queryClient.resetQueries();
      queryClient.setQueryData(["status"], status);
    } catch (e) {
      return fail(`Sign-in did not work. ${e instanceof Error ? e.message : String(e)}`);
    }
    // Today opens at once. The import runs on and shows its progress there.
    importHistory();
    finish();
  };

  const fail = (text: string) => {
    setProblem(text);
    setStep("failed");
    used.current = false;
  };

  if (url.error) return <Message>{`The sign-in page is not available. ${url.error}`}</Message>;
  if (!url.data) return <Message>Loading the sign-in page.</Message>;
  if (step === "redeeming") return <Message>Signing in. This takes a few seconds.</Message>;
  if (step === "failed") {
    return (
      <Message action={<Button title="Try again" onPress={() => setStep("page")} />}>
        {problem}
      </Message>
    );
  }
  return (
    <WebView
      source={{ uri: url.data }}
      onShouldStartLoadWithRequest={(request) => {
        const ticket = ticketIn(request.url);
        if (!ticket) return true;
        if (!used.current) {
          used.current = true;
          start(ticket);
        }
        return false;
      }}
    />
  );
}
