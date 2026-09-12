// A declared stand-in for the four cocos2d-x facilities the recorder node
// uses, so the node compiles and runs in continuous integration without a
// cocos2d-x tree, which is hundreds of megabytes and a long build.
//
// This is not cocos2d-x and does not pretend to be. It exists so the
// adapter's own logic is exercised against the real native library, and
// so a signature drift in the adapter fails a build rather than waiting
// for a user's project to find it. Validation against the engine itself
// is a separate step, described in the README.
//
// Each declaration mirrors the shape that has been stable across
// cocos2d-x 3.x and 4.0:
//   - cocos2d::Ref::autorelease and cocos2d::Node with virtual init,
//     onEnter, onExit, and a virtual destructor
//   - cocos2d::FileUtils::getInstance()->getWritablePath()
//   - cocos2d::log(const char*, ...)

#ifndef TICKWISE_COCOS2D_SHIM_H
#define TICKWISE_COCOS2D_SHIM_H

#include <cstdarg>
#include <cstdio>
#include <string>

namespace cocos2d {

class Ref {
public:
    virtual ~Ref() {}
    /** The real engine defers deletion to the autorelease pool. The shim
     *  has no pool, so the test releases the node itself, through the
     *  same call the engine uses. */
    void autorelease() {}
    void retain() {}
    void release() { delete this; }
};

class Node : public Ref {
public:
    virtual bool init() { return true; }
    virtual void onEnter() {}
    virtual void onExit() {}
};

class FileUtils {
public:
    static FileUtils* getInstance() {
        static FileUtils instance;
        return &instance;
    }
    std::string getWritablePath() const { return writablePath; }
    /** Test hook: where relative recording paths resolve. */
    std::string writablePath;
};

inline void log(const char* format, ...) {
    va_list args;
    va_start(args, format);
    std::vfprintf(stdout, format, args);
    va_end(args);
    std::fputc('\n', stdout);
}

} // namespace cocos2d

#endif // TICKWISE_COCOS2D_SHIM_H
