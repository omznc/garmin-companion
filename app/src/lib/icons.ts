/**
 * Every icon the app uses, named for its job here rather than for its drawing.
 *
 * One module for three reasons. The set stays small and visible — adding an
 * icon means adding a line here, which is a decision you make once rather than
 * an import you sneak into a screen. Renaming at the boundary means a screen
 * says `<NavToday />`, not `<House />`, so swapping the glyph later doesn't
 * touch the screens. And the imports are per-icon paths into `dist/csr`, not
 * the package barrel: the barrel is ~1,500 modules and the dev server pays for
 * every one of them on first load.
 *
 * Weight and size are not set here — `main.tsx` puts `duotone` at 16px on the
 * shared `IconContext`, so anything rendered without props is already right and
 * the odd exception is visible where it's made.
 */

/* ---------------------------------------------------------------- chrome --- */

export { XIcon as CloseIcon } from "@phosphor-icons/react/dist/csr/X";
export { MinusIcon as MinimiseIcon } from "@phosphor-icons/react/dist/csr/Minus";
export { SquareIcon as MaximiseIcon } from "@phosphor-icons/react/dist/csr/Square";
export { CopySimpleIcon as RestoreIcon } from "@phosphor-icons/react/dist/csr/CopySimple";

/* -------------------------------------------------------------- movement --- */

export { ArrowRightIcon } from "@phosphor-icons/react/dist/csr/ArrowRight";
export { CaretLeftIcon as BackIcon } from "@phosphor-icons/react/dist/csr/CaretLeft";
export { ArrowsClockwiseIcon as SyncIcon } from "@phosphor-icons/react/dist/csr/ArrowsClockwise";
export { CircleNotchIcon as SpinnerIcon } from "@phosphor-icons/react/dist/csr/CircleNotch";

/* --------------------------------------------------------------- actions --- */

export { PushPinIcon as PinIcon } from "@phosphor-icons/react/dist/csr/PushPin";
export { PushPinSlashIcon as UnpinIcon } from "@phosphor-icons/react/dist/csr/PushPinSlash";
export { TrashIcon as DeleteIcon } from "@phosphor-icons/react/dist/csr/Trash";
export { PaperPlaneRightIcon as SendIcon } from "@phosphor-icons/react/dist/csr/PaperPlaneRight";
export { PlusIcon as NewIcon } from "@phosphor-icons/react/dist/csr/Plus";
export { DownloadSimpleIcon as UpdateIcon } from "@phosphor-icons/react/dist/csr/DownloadSimple";
export { ArrowsCounterClockwiseIcon as FullSyncIcon } from "@phosphor-icons/react/dist/csr/ArrowsCounterClockwise";
export { LinkBreakIcon as DisconnectIcon } from "@phosphor-icons/react/dist/csr/LinkBreak";
export { ArrowSquareOutIcon as ExternalIcon } from "@phosphor-icons/react/dist/csr/ArrowSquareOut";
export { ArrowUpIcon as MoveUpIcon } from "@phosphor-icons/react/dist/csr/ArrowUp";
export { ArrowDownIcon as MoveDownIcon } from "@phosphor-icons/react/dist/csr/ArrowDown";
export { SunIcon as LightIcon } from "@phosphor-icons/react/dist/csr/Sun";
export { MoonIcon as DarkIcon } from "@phosphor-icons/react/dist/csr/Moon";
export { PencilSimpleIcon as EditIcon } from "@phosphor-icons/react/dist/csr/PencilSimple";
export { FolderOpenIcon as FolderIcon } from "@phosphor-icons/react/dist/csr/FolderOpen";
/* The phone tab bar's fourth tab, opening the sheet with the screens that
 * didn't get one. Dots rather than a hamburger: the bar's other three are
 * destinations, and a hamburger promises a drawer sliding in from the side.
 * Circled, because the other three tabs are all closed shapes at 21px and a
 * bare row of dots read as an ellipsis dropped into the bar rather than as the
 * fourth member of a set. */
export { DotsThreeCircleIcon as MoreIcon } from "@phosphor-icons/react/dist/csr/DotsThreeCircle";

/* Marks a region of the screen a model wrote, in development builds only.
 * See `components/AiMark.tsx` for why that boundary is worth being able to
 * see. */
export { SparkleIcon as AiIcon } from "@phosphor-icons/react/dist/csr/Sparkle";

/* ---------------------------------------------------------------- states --- */

export { WarningCircleIcon as ErrorIcon } from "@phosphor-icons/react/dist/csr/WarningCircle";
export { CheckCircleIcon as DoneIcon } from "@phosphor-icons/react/dist/csr/CheckCircle";

/* ------------------------------------------------------------------- nav --- */

/* The twelve screens, in sidebar order. A running app leans on a few of these
 * heavily, so they're picked to be distinguishable at 15px from each other
 * first and to be literal second — `SneakerMove` for activities over a stopwatch,
 * because a stopwatch reads as "timer" next to a heart that means "health". */
export { HouseIcon as NavToday } from "@phosphor-icons/react/dist/csr/House";
export { SneakerMoveIcon as NavActivities } from "@phosphor-icons/react/dist/csr/SneakerMove";
export { HeartbeatIcon as NavHealth } from "@phosphor-icons/react/dist/csr/Heartbeat";
export { ForkKnifeIcon as NavFood } from "@phosphor-icons/react/dist/csr/ForkKnife";
export { ScalesIcon as NavWeight } from "@phosphor-icons/react/dist/csr/Scales";
export { ChatCircleDotsIcon as NavAsk } from "@phosphor-icons/react/dist/csr/ChatCircleDots";
export { LightbulbIcon as NavInsights } from "@phosphor-icons/react/dist/csr/Lightbulb";
export { CalendarBlankIcon as NavPlan } from "@phosphor-icons/react/dist/csr/CalendarBlank";
export { MapTrifoldIcon as NavRoutes } from "@phosphor-icons/react/dist/csr/MapTrifold";
export { BackpackIcon as NavGear } from "@phosphor-icons/react/dist/csr/Backpack";
export { BarbellIcon as NavStrength } from "@phosphor-icons/react/dist/csr/Barbell";
export { GaugeIcon as NavFitness } from "@phosphor-icons/react/dist/csr/Gauge";
export { FileTextIcon as NavReports } from "@phosphor-icons/react/dist/csr/FileText";
export { GearSixIcon as NavSettings } from "@phosphor-icons/react/dist/csr/GearSix";
