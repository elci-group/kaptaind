#import <Foundation/Foundation.h>

@protocol GreeterDelegate <NSObject>

- (void)greeterDidFinish:(id)sender;
@optional
- (BOOL)greeterShouldContinue;

@end
