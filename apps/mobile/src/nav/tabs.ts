// The five tabs. Settings live in More.
export const TABS = [
  { name: "today", title: "Today", sf: "sun.max", sfSelected: "sun.max.fill" },
  { name: "activities", title: "Activities", sf: "figure.run", sfSelected: "figure.run" },
  { name: "health", title: "Health", sf: "heart", sfSelected: "heart.fill" },
  { name: "ask", title: "Ask", sf: "bubble.left", sfSelected: "bubble.left.fill" },
  { name: "more", title: "More", sf: "ellipsis.circle", sfSelected: "ellipsis.circle.fill" },
] as const;
