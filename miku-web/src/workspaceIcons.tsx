import { ArrowUp } from "@phosphor-icons/react/dist/icons/ArrowUp";
import { ArrowUpRight } from "@phosphor-icons/react/dist/icons/ArrowUpRight";
import { BookOpen } from "@phosphor-icons/react/dist/icons/BookOpen";
import { CaretDown } from "@phosphor-icons/react/dist/icons/CaretDown";
import { CaretLeft } from "@phosphor-icons/react/dist/icons/CaretLeft";
import { CaretRight } from "@phosphor-icons/react/dist/icons/CaretRight";
import { CheckCircle } from "@phosphor-icons/react/dist/icons/CheckCircle";
import { Clock } from "@phosphor-icons/react/dist/icons/Clock";
import { FileText } from "@phosphor-icons/react/dist/icons/FileText";
import { Folder } from "@phosphor-icons/react/dist/icons/Folder";
import { GearSix } from "@phosphor-icons/react/dist/icons/GearSix";
import { Hash } from "@phosphor-icons/react/dist/icons/Hash";
import { MagnifyingGlass } from "@phosphor-icons/react/dist/icons/MagnifyingGlass";
import { Moon } from "@phosphor-icons/react/dist/icons/Moon";
import { Rocket } from "@phosphor-icons/react/dist/icons/Rocket";
import { Sun } from "@phosphor-icons/react/dist/icons/Sun";
import { TreeStructure } from "@phosphor-icons/react/dist/icons/TreeStructure";
import { X } from "@phosphor-icons/react/dist/icons/X";
import type { Icon } from "@phosphor-icons/react";

export type ActionIconName = "arrow-up" | "arrow-up-right" | "chevron-down" | "chevron-left" | "chevron-right" | "close" | "hash" | "moon" | "search" | "settings" | "sun" | "tree" | "clock";

export function ActionIcon({ name }: { name: ActionIconName }) {
  const icons: Record<ActionIconName, Icon> = {
    "arrow-up": ArrowUp,
    "arrow-up-right": ArrowUpRight,
    "chevron-down": CaretDown,
    "chevron-left": CaretLeft,
    "chevron-right": CaretRight,
    close: X,
    hash: Hash,
    moon: Moon,
    search: MagnifyingGlass,
    settings: GearSix,
    sun: Sun,
    tree: TreeStructure,
    clock: Clock
  };
  const IconComponent = icons[name];
  return <IconComponent className="action-icon" size={16} weight="regular" aria-hidden="true" />;
}

export function NoteIcon({ value = "file-text", large = false }: { value?: string; large?: boolean }) {
  const icon = value.trim();
  const isImage = /^(https?:\/\/|\/assets\/)/.test(icon);
  if (isImage) return <img className={`note-icon-image ${large ? "is-large" : ""}`} src={icon} alt="" />;
  const icons: Record<string, Icon> = { "file-text": FileText, note: FileText, book: BookOpen, "check-circle": CheckCircle, rocket: Rocket, folder: Folder };
  const IconComponent = icons[icon.toLowerCase()] ?? FileText;
  const isEmoji = !icons[icon.toLowerCase()] && !/^[a-z0-9-]+$/i.test(icon);
  return isEmoji ? <span className={`note-icon-emoji ${large ? "is-large" : ""}`} aria-hidden="true">{icon}</span> : <IconComponent className={`note-icon-library ${large ? "is-large" : ""}`} size={large ? 25 : 16} weight="regular" aria-hidden="true" />;
}
