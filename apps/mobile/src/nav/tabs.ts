// The four tabs. Settings is not a tab: each tab opens it from the top bar.
export const TABS = [
  { name: "today", title: "Today", sf: "sun.max", sfSelected: "sun.max.fill" },
  { name: "activities", title: "Activities", sf: "figure.run", sfSelected: "figure.run" },
  { name: "ask", title: "Ask", sf: "bubble.left", sfSelected: "bubble.left.fill" },
  { name: "health", title: "Health", sf: "heart", sfSelected: "heart.fill" },
] as const;
