-- This file contains only comments; expect zero public symbols.
-- The next lines look like declarations but are commented out:
-- data Foo = MkFoo
-- type Bar = Int
-- secret = 42
{- A block comment spanning lines.
   It mentions data and class but not at the start of a line,
   so the scanner must not treat them as declarations. -}
-- done
