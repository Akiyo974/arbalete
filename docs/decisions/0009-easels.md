# 0009 — Easels

- Status: accepted
- Date: 2026-11-09

## Context

Arc's Easels are boards that live in the sidebar like tabs: the user lays out captures of pages, pictures,
texts, shapes, arrows and pen strokes, and shares the board. Arbalete has the capture tool and its editor
(`Capture.swift`, `CaptureEditor.swift`), but no board to keep captures on.

What Arc's app shows of them (its binary and the web export it ships, studied without running it):

- An Easel is a list of objects, back to front, on a board 1000 points wide that the view scales to its width
  and that grows as tall as its objects reach. Objects: a capture of a page (its picture, its address and
  title, and where on the page it was), a picture, a text (a face and a size), a shape (rectangle or ellipse,
  translucent on an edge of its color), an arrow and a pen stroke, their color from a small palette.
- A tool bar picks the pointer, a picture, a text, a shape, an arrow or the pen; the selected object moves,
  resizes, recolors, and copies or pastes.
- An Easel is a sidebar item with its own icon, made from the "+" menu, the Command Bar or the capture tool's
  menu ("add to an Easel"); it archives and closes like a tab.
- A capture can turn live: the page loads at the size it was captured, scrolls to where it was and shows only
  that part, so it stays up to date.
- Easels live in Arc's cloud and are shared by a link that any browser opens. Arc now also exports them as a
  folder of web pages, each with its board as JSON.

Arbalete has no server.

## Decision

A first version of local Easels, with live captures and Arc's look, without sharing.

- **Objects**: captures of a page as they looked, pictures, texts, rectangles, ellipses, arrows and pen
  strokes. The board is 1000 points wide, its objects' boxes in its points; an arrow's or a stroke's points
  are fractions of its box, so resizing stretches them.
- **Storage**: one folder per Easel, `Easels/<number>` in the data folder, holding `easel.json` (a format
  version and the objects) and `images/`, the pictures its objects show. `arbalete-core` (`easel.rs`) loads and
  saves it; a save writes atomically, refuses a picture missing from `images/` or a name that leaves it, and
  deletes the pictures no object shows any more.
- **In the sidebar**: an Easel is a tab whose address is `arbalete://easel/<number>`, named like a tab
  ("Untitled Easel" until renamed) and shown with a drawing icon instead of a favicon. Being a tab, it pins,
  moves into folders and favorites, archives from "Today", closes, reopens with ⇧⌘T, restores from the archive
  and shows in the Command Bar's tab search without new code. Like Arc's, it goes into a split, beside a
  page or another Easel; it is not duplicated. An Easel folder no tab, closed tab or archived tab refers to
  any more is deleted at launch, like unused favicons.
- **Making one**: "New Easel" in the sidebar's "+" menu (⌃⇧E) and in the Command Bar opens an empty Easel in
  "Today", selected, its title ready to type. The capture tool's menu gains "Add to Easel", which lists the
  displayed Space's Easels, most recent first, and "New Easel": like Arc's, the part picked lands in the first
  slot of a three-a-row grid that no object touches, as large as the slot allows, with the page's address
  and title, and the Easel opens in Peek, scrolled to it.
- **The board**: a native view in the page's place on the card (AppKit, like the capture editor), not a web
  page: the board scrolls up and down, scaled to the card's width. It reuses the capture editor's tools and
  drawing; a click on nothing deselects; Delete removes the selection; ⌘C, ⌘V and ⌘Z work on objects; a
  picture pasted or dropped from Finder lands where the pointer is.
- **Arc's look**, from the values its app ships:
  - the board is the window's color darkened to 46%, measured on Arc's, under light marks and tools;
  - an object dragged snaps, unless ⌘ is held, to the board's margins and middle, to the same edges and
    middles of the objects near it, and 24 points beside them, near-white guides and blue marks showing it,
    with the reach and stickiness Arc's code holds;
  - colors are Arc's "chill" palette by name (black, white, grey, pink, red, yellow, light and dark green,
    light and dark blue, purple), black and white swapping in dark mode; a new rectangle is light green, an
    ellipse red, an arrow light blue, a pen stroke black;
  - shapes fill at 75% of their color with a 2.5-point edge of it; an arrow is a line with two 45° flares;
  - texts take one of four faces, Arc's: the system's ("clean"), Inter ("hip"), Space Mono ("mono") and EB
    Garamond ("fancy"), in five sizes (from body to "ultra"), a body text aligned left and a title centered,
    in black (white in dark mode), shrinking to fit their box. The three fonts are free (SIL Open Font
    License) and ship with Arbalete, from their own projects, with their license;
  - a selection is outlined in Arc's blue (#6cbad9);
  - a capture is a card with 10-point corners and a hairline edge; hovered, a bar along its bottom shows its
    page's icon and title, a button opening the page in a new tab, and one turning it live or back.
- **Who holds what**: the board's view keeps the Easel it shows while editing and sends it whole to the app
  after each change; the app saves it, and a picture added goes in first, by its own command. Showing an Easel
  loads it from its file, so another window's edits show the next time it shows there.

- **Live captures**, like Arc's: a capture keeps the size of the page's view, where it was scrolled and the
  part picked. Turned live (asked once, the first time), it shows the page itself, loaded in the Easel's
  profile at that size, scrolled there and cut to that part, scaled to the card, so it stays up to date. It
  only shows: clicks go to the board, and its bar opens the page. Its page is muted, and closes while the
  Easel is not shown, loading again when it shows.

Later, each in a change of its own:

- **Sharing**: no sharing at first. Later, an export as a self-contained web page folder that any static host
  serves.
- **Arc's Easels**: importing the folders Arc exports (their `easel.json`) once the format is pinned down.

## Consequences

- Easels need no change to the sidebar's model or to the session file: an Easel is a tab with an internal
  address, and its content lives apart, so a broken Easel file cannot break the session.
- The places that assume a tab has a web page (its menu, copy link, the duplicate) check for an
  Easel's address.
- Two windows showing the same Easel at once each save their own copy: the last change wins. An Easel shows
  in one window most of the time, like a page.
- A board's pictures stay on disk while any tab, closed tab or archived tab refers to it.
- Each live capture shown is a page of its own, loaded each time the Easel shows: an Easel full of them weighs
  like as many tabs while it shows.
- About 1.5 MB of fonts ship with the app, and their license with them.
