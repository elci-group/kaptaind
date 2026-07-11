clamp :: Int -> Int -> Int -> Int
clamp lo hi x
    | x < lo    = lo
    | x > hi    = hi
    | otherwise = x
