#pragma once
class Screen;
struct Game;

// Presents the board until TuxBox's performance panel is on screen over it, and
// throws if it does not appear. The launch must set TUXBOX_OVERLAY=1.
void expect_performance_panel(Screen& screen, const Game& game);
