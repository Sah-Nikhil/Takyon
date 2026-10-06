/**
 * One Focus category's page, "Focus / Social": everything it blocks, which of
 * its apps this machine has, and an adder. Built-ins take additions and
 * removals as edits over the shipped list (`categories.rs` `with_edits`),
 * with Reset to undo them; custom ones are edited outright.
 */

import { useCallback, useEffect, useState } from "react";
import { AppWindow, LinkSimple, X } from "@phosphor-icons/react";
import type { FocusCategory, FocusItem, FocusMember } from "@takyon/shared";
import * as api from "@/api";
import { BlockPicker } from "@/focus/BlockPicker";
import { FocusIcon, PICKABLE_ICONS } from "@/focus/icons";
import { Confirm, Group } from "../controls";

const small =
  "rounded-control px-2.5 py-1 text-[12.5px] text-fg/80 transition-colors hover:bg-row-hover hover:text-fg";
const onHover = "opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100";

function count(n: number, one: string, many: string) {
  return `${n} ${n === 1 ? one : many}`;
}

/** "Discord.exe", "Store app", "Steam game": where an app item is found. */
function source(m: FocusMember): string {
  if (m.detected) return "Found through its launcher";
  const [kind, rest = ""] = m.key.split(":");
  if (kind === "exe") return rest;
  if (kind === "package") return "Microsoft Store app";
  return "Picked from your apps";
}

function MemberRow({ member, onRemove }: { member: FocusMember; onRemove: () => void }) {
  const url = member.icon ? api.iconUrl(member.icon) : "";
  return (
    <div className="group flex items-center gap-3 px-3.5 py-2">
      {member.kind === "site" ? (
        <LinkSimple weight="duotone" className="size-4 shrink-0 text-fg/55" aria-hidden />
      ) : url ? (
        <img src={url} alt="" className="size-4 shrink-0" />
      ) : (
        <AppWindow weight="duotone" className="size-4 shrink-0 text-fg/55" aria-hidden />
      )}
      <span className="min-w-0 truncate text-[13px] text-fg">{member.label}</span>
      {member.added && <span className="rounded bg-control px-1.5 py-px text-[10.5px] text-fg/60">Added</span>}
      <span className="min-w-0 flex-1 truncate text-[12px] text-fg/45">
        {member.kind === "site" ? (member.key.slice(5) === member.label ? "" : member.key.slice(5)) : source(member)}
      </span>
      {member.kind === "app" && (
        <span className={`shrink-0 text-[11.5px] ${member.installed ? "text-fg/65" : "text-fg/40"}`}>
          {member.installed ? "Installed" : "Not installed"}
        </span>
      )}
      <button
        type="button"
        aria-label={`Remove ${member.label}`}
        onClick={onRemove}
        className={`grid size-6 shrink-0 place-items-center rounded-control text-fg/55 hover:bg-row-hover hover:text-fg ${onHover}`}
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}

export function FocusCategoryPanel({
  id,
  onOpen,
  onGone,
  onRename,
}: {
  id: string;
  /** Show another category's page (after Duplicate). */
  onOpen: (category: FocusCategory) => void;
  /** This category no longer exists (deleted). */
  onGone: () => void;
  /** Saved under a new name: the title follows. */
  onRename: (name: string) => void;
}) {
  const [category, setCategory] = useState<FocusCategory | null>(null);
  const [members, setMembers] = useState<FocusMember[]>([]);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<"reset" | "delete" | null>(null);
  const builtin = !id.startsWith("custom:");

  const load = useCallback(() => {
    void api.focusCategories().then((all) => {
      const c = [...all.builtins, ...all.custom].find((x) => x.id === id) ?? null;
      setCategory(c);
      if (c) setName(c.name);
    });
    void api.focusMembers(id).then(setMembers);
  }, [id]);
  useEffect(load, [load]);

  if (!category) return null;

  const saveCustom = (next: FocusCategory) =>
    api.focusSaveCategory(next).then(
      () => (setError(null), load()),
      (e) => setError(String(e)),
    );

  const add = (key: string, label: string) => {
    if (builtin) void api.focusEditBuiltin(id, { key, label }, false).then(load, (e) => setError(String(e)));
    else if (!category.items.some((i) => i.key === key)) void saveCustom({ ...category, items: [...category.items, { key, label }] });
  };

  const remove = (item: FocusItem) => {
    if (builtin) void api.focusEditBuiltin(id, item, true).then(load);
    else void saveCustom({ ...category, items: category.items.filter((i) => i.key !== item.key) });
  };

  const rename = () => {
    const next = name.trim();
    if (!next) return setName(category.name);
    if (next !== category.name) void saveCustom({ ...category, name: next }).then(() => onRename(next));
  };

  const duplicate = () =>
    void api
      .focusSaveCategory({
        ...category,
        id: "",
        name: `${category.name} (copy)`,
        dynamic: null,
        copiedFrom: category.id,
        added: [],
        removed: [],
      })
      .then(onOpen);

  const exportOne = () =>
    void api.focusExportCategories([id]).then((json) => {
      const url = URL.createObjectURL(new Blob([json], { type: "application/json" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = `takyon-focus-${category.name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}.json`;
      a.click();
      URL.revokeObjectURL(url);
    });

  const listed = members.filter((m) => !m.detected);
  const apps = listed.filter((m) => m.kind === "app");
  const sites = listed.filter((m) => m.kind === "site");
  const games = members.filter((m) => m.detected);
  const installed = apps.filter((m) => m.installed).length + games.length;
  const edited = (category.added?.length ?? 0) + (category.removed?.length ?? 0) > 0;
  const summary = [
    count(sites.length, "website", "websites"),
    count(apps.length + games.length, "app", "apps"),
    `${installed} on this PC`,
  ].join(" · ");

  return (
    <div className="space-y-6">
      <div className="flex items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-card border border-hairline bg-card">
          <FocusIcon name={category.icon} className="size-5 text-accent" />
        </span>
        <div className="min-w-0 flex-1">
          {builtin ? (
            <h2 className="text-[15px] font-medium text-fg">{category.name}</h2>
          ) : (
            <input
              value={name}
              aria-label="Focus category name"
              onChange={(e) => setName(e.target.value)}
              onBlur={rename}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="-ml-1.5 w-full max-w-sm rounded-control bg-transparent px-1.5 py-0.5 text-[15px] font-medium text-fg outline-none hover:bg-row-hover focus:bg-control"
            />
          )}
          <p className="mt-0.5 text-[12px] text-fg/55">
            {summary}
            {builtin && " · Built in"}
          </p>
          {!builtin && (
            <div role="radiogroup" aria-label="Icon" className="mt-2 flex flex-wrap gap-0.5">
              {PICKABLE_ICONS.map((icon) => (
                <button
                  key={icon}
                  type="button"
                  role="radio"
                  aria-checked={category.icon === icon}
                  aria-label={icon}
                  onClick={() => void saveCustom({ ...category, icon })}
                  className={`grid size-7 place-items-center rounded-control ${
                    category.icon === icon ? "bg-card ring-1 ring-accent/45" : "hover:bg-row-hover"
                  }`}
                >
                  <FocusIcon name={icon} className="size-4 text-accent" />
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="flex shrink-0 gap-1">
          {builtin && edited && (
            <button type="button" className={small} onClick={() => setConfirm("reset")}>
              Reset to default
            </button>
          )}
          <button type="button" className={small} onClick={duplicate}>
            Duplicate
          </button>
          {!builtin && (
            <>
              <button type="button" className={small} onClick={exportOne}>
                Export
              </button>
              <button type="button" className={small} onClick={() => setConfirm("delete")}>
                Delete
              </button>
            </>
          )}
        </div>
      </div>

      <div>
        <BlockPicker
          value={{ categories: [], items: [] }}
          categories={[]}
          onPick={add}
          placeholder={`Add an app or a website to ${category.name}…`}
        />
        {error && (
          <p className="mt-1.5 text-[12px] text-warning" role="alert">
            {error}
          </p>
        )}
      </div>

      {apps.length > 0 && (
        <Group title="Apps">
          {apps.map((m) => (
            <MemberRow key={m.key} member={m} onRemove={() => remove(m)} />
          ))}
        </Group>
      )}
      {games.length > 0 && (
        <Group title="Installed games">
          {games.map((m) => (
            <MemberRow key={m.key} member={m} onRemove={() => remove(m)} />
          ))}
        </Group>
      )}
      {category.dynamic === "games" && games.length === 0 && (
        <p className="px-1 text-[12px] text-fg/50">
          No games found through Steam or Epic. Games installed later are blocked with this category too.
        </p>
      )}
      {sites.length > 0 && (
        <Group title="Websites">
          {sites.map((m) => (
            <MemberRow key={m.key} member={m} onRemove={() => remove(m)} />
          ))}
        </Group>
      )}
      {listed.length === 0 && games.length === 0 && category.dynamic !== "games" && (
        <p className="px-1 text-[12px] text-fg/50">Nothing in this Focus category yet. Add apps or websites above.</p>
      )}
      {builtin && (category.removed?.length ?? 0) > 0 && (
        <Group title="Taken out">
          {category.removed!.map((item) => (
            <div key={item.key} className="flex items-center gap-3 px-3.5 py-2">
              <span className="min-w-0 flex-1 truncate text-[13px] text-fg/60">{item.label}</span>
              <button
                type="button"
                className={small}
                onClick={() => void api.focusEditBuiltin(id, item, false).then(load)}
              >
                Put back
              </button>
            </div>
          ))}
        </Group>
      )}

      {confirm === "reset" && (
        <Confirm
          title={`Reset ${category.name}?`}
          consequence="What you added goes and what you took out comes back."
          confirmLabel="Reset to default"
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            setConfirm(null);
            void api.focusResetBuiltin(id).then(load);
          }}
        />
      )}
      {confirm === "delete" && (
        <Confirm
          title={`Delete ${category.name}?`}
          consequence="Its apps and websites are no longer blocked from the next session on."
          confirmLabel="Delete"
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            setConfirm(null);
            void api.focusDeleteCategory(id).then(onGone);
          }}
        />
      )}
    </div>
  );
}
