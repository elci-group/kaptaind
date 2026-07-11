module Main where

add :: Int -> Int -> Int
add x y = x + y

data Point = Point Int Int

newtype Name = Name String

class Showable a where

type Alias = Int
