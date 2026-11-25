# Scan

Scans: a plan of tiles over a region, run into a project, and stitched. On the left, its panes; on the right, the plan over the slide, and the mosaic as it fills.

## Plan a scan

1. **The pattern:**
   - a *snake* or a *raster* over a region;
   - a *z-scan*: a stack of planes at one place;
   - a *tiled stack*: a stack at each tile;
   - *positions*: places added where the stage is, each visited in its order;
   - the patterns extensions add.
2. **The region,** for the patterns that cover one: type x, y, width and height, or press *Draw* and drag on the view. The overlap between tiles is chosen beside it.
3. **The focus:**
   - where the stage is;
   - autofocus every so many tiles;
   - a focus map, measured before the first tile.
4. **The estimate:** the tiles, frames, time and data, and any warning: tiles past the stage's limits, the geometry not calibrated, not enough room.
5. **The run:** choose or make a project, name the scan, and *Run the scan*. The tiles turn green as they are taken; the next is marked. *Pause* holds the stage between tiles, *Resume* goes on, and *Cancel* stops after the step under way. What was taken stays in scratch.
6. **At its end:** *Open it in the Gallery*, and *Stitch it*: the stitch job's state, then *Open the stitched image*.

## The histo scan

A slide's tissue, found and scanned, in five steps along the top:

1. **The overview:** type the slide's region and *Make the overview*: its frames, then the slide on the view.
2. **The tissue:** the islands found, their area, and the threshold. *Propose the regions again* after a new overview.
3. **The regions:** one about each island. Untick *Keep it* to leave one out, drawn fainter; *Draw it again* to drag a new rectangle; *Draw a new one* for another.
4. **The focus map:** its columns and rows, and its surface, a plane or a thin plate. Its points show on the view.
5. **The run,** as above. At its end, *The focus map* says its surface, how well it fits, and each point's focus.
