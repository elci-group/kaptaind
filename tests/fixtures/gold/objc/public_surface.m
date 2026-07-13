#import <Foundation/Foundation.h>

@interface Greeter : NSObject

@property (nonatomic, copy) NSString *name;

- (void)greet:(NSString *)target withPunctuation:(BOOL)punct;
+ (instancetype)sharedGreeter;
- (void)_resetCache;

@end
