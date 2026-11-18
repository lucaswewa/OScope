// The icons the app uses (ADR-0041): Material Symbols, outlined, at weight 300.
// Each is an SVG file from the package, read into the bundle as text: none is
// fetched at run time. An icon the app needs is added here, by the name the
// app knows it by.
import add from "@material-symbols/svg-300/outlined/add.svg?raw";
import remove from "@material-symbols/svg-300/outlined/remove.svg?raw";
import close from "@material-symbols/svg-300/outlined/close.svg?raw";
import check from "@material-symbols/svg-300/outlined/check.svg?raw";
import search from "@material-symbols/svg-300/outlined/search.svg?raw";
import refresh from "@material-symbols/svg-300/outlined/refresh.svg?raw";
import download from "@material-symbols/svg-300/outlined/download.svg?raw";
import upload from "@material-symbols/svg-300/outlined/upload.svg?raw";
import deleted from "@material-symbols/svg-300/outlined/delete.svg?raw";
import edit from "@material-symbols/svg-300/outlined/edit.svg?raw";
import more from "@material-symbols/svg-300/outlined/more_horiz.svg?raw";
import menu from "@material-symbols/svg-300/outlined/menu.svg?raw";
import up from "@material-symbols/svg-300/outlined/arrow_upward.svg?raw";
import down from "@material-symbols/svg-300/outlined/arrow_downward.svg?raw";
import back from "@material-symbols/svg-300/outlined/arrow_back.svg?raw";
import forward from "@material-symbols/svg-300/outlined/arrow_forward.svg?raw";
import expand from "@material-symbols/svg-300/outlined/keyboard_arrow_down.svg?raw";
import collapse from "@material-symbols/svg-300/outlined/keyboard_arrow_up.svg?raw";
import previous from "@material-symbols/svg-300/outlined/chevron_left.svg?raw";
import next from "@material-symbols/svg-300/outlined/chevron_right.svg?raw";
import info from "@material-symbols/svg-300/outlined/info.svg?raw";
import success from "@material-symbols/svg-300/outlined/check_circle.svg?raw";
import warning from "@material-symbols/svg-300/outlined/warning.svg?raw";
import danger from "@material-symbols/svg-300/outlined/error.svg?raw";
import help from "@material-symbols/svg-300/outlined/help.svg?raw";
import show from "@material-symbols/svg-300/outlined/visibility.svg?raw";
import hide from "@material-symbols/svg-300/outlined/visibility_off.svg?raw";
import camera from "@material-symbols/svg-300/outlined/photo_camera.svg?raw";
import settings from "@material-symbols/svg-300/outlined/settings.svg?raw";
import power from "@material-symbols/svg-300/outlined/power_settings_new.svg?raw";
import home from "@material-symbols/svg-300/outlined/home.svg?raw";
import folder from "@material-symbols/svg-300/outlined/folder.svg?raw";
import file from "@material-symbols/svg-300/outlined/draft.svg?raw";
import pause from "@material-symbols/svg-300/outlined/pause.svg?raw";
import play from "@material-symbols/svg-300/outlined/play_arrow.svg?raw";
import light from "@material-symbols/svg-300/outlined/light_mode.svg?raw";
import dark from "@material-symbols/svg-300/outlined/dark_mode.svg?raw";
import computer from "@material-symbols/svg-300/outlined/contrast.svg?raw";

/** The box every Material Symbols file draws in. */
export const BOX = "0 -960 960 960";

/** A file's one path. A file in another form is refused when the app starts, not drawn wrong. */
export function pathOf(svg: string): string {
  const d = / d="([^"]+)"/.exec(svg)?.[1];
  if (!d || !svg.includes(`viewBox="${BOX}"`) || svg.split("<path").length !== 2) {
    throw new Error(`not a Material Symbols icon: ${svg.slice(0, 80)}`);
  }
  return d;
}

/** Each icon's path, by its name. */
export const icons = {
  add: pathOf(add),
  remove: pathOf(remove),
  close: pathOf(close),
  check: pathOf(check),
  search: pathOf(search),
  refresh: pathOf(refresh),
  download: pathOf(download),
  upload: pathOf(upload),
  delete: pathOf(deleted),
  edit: pathOf(edit),
  more: pathOf(more),
  menu: pathOf(menu),
  up: pathOf(up),
  down: pathOf(down),
  back: pathOf(back),
  forward: pathOf(forward),
  expand: pathOf(expand),
  collapse: pathOf(collapse),
  previous: pathOf(previous),
  next: pathOf(next),
  info: pathOf(info),
  success: pathOf(success),
  warning: pathOf(warning),
  danger: pathOf(danger),
  help: pathOf(help),
  show: pathOf(show),
  hide: pathOf(hide),
  camera: pathOf(camera),
  settings: pathOf(settings),
  power: pathOf(power),
  home: pathOf(home),
  folder: pathOf(folder),
  file: pathOf(file),
  pause: pathOf(pause),
  play: pathOf(play),
  light: pathOf(light),
  dark: pathOf(dark),
  system: pathOf(computer),
} as const;

/** An icon the registry has. */
export type IconName = keyof typeof icons;
